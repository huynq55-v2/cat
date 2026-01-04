use shared::serial_println;
use spin::Mutex;
use x86_64::PhysAddr;
use x86_64::instructions::interrupts;
use x86_64::structures::paging::{FrameAllocator, PhysFrame, Size4KiB};

pub const PAGE_SIZE: u64 = 4096;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct MemoryDescriptor {
    pub type_: u32,
    pub pad: u32,
    pub phys_start: u64,
    pub virt_start: u64,
    pub page_count: u64,
    pub attribute: u64,
}

impl MemoryDescriptor {
    pub fn type_name(&self) -> &'static str {
        match self.type_ {
            0 => "Reserved",
            1 => "LoaderCode",
            2 => "LoaderData",
            3 => "BootServicesCode",
            4 => "BootServicesData",
            5 => "RuntimeServicesCode",
            6 => "RuntimeServicesData",
            7 => "ConventionalMemory",
            8 => "UnusableMemory",
            9 => "ACPIReclaimMemory",
            10 => "ACPIMemoryNVS",
            11 => "MemoryMappedIO",
            12 => "MemoryMappedIOPortSpace",
            13 => "PalCode",
            14 => "PersistentMemory",
            _ => "Unknown",
        }
    }
}

unsafe impl Send for BitmapPmm {}

struct BitmapPmm {
    bitmap: *mut u64,
    total_frames: usize,
    bitmap_size_u64: usize,
    bitmap_start_addr: u64,
}

static PMM: Mutex<BitmapPmm> = Mutex::new(BitmapPmm {
    bitmap: core::ptr::null_mut(),
    total_frames: 0,
    bitmap_size_u64: 0,
    bitmap_start_addr: 0,
});

impl BitmapPmm {
    unsafe fn init_internal(
        &mut self,
        mmap_addr_phys: u64,
        mmap_len: u64,
        desc_size: u64,
        hhdm_offset: u64,
        max_phys_addr: u64,
    ) {
        let mmap_addr_virt = mmap_addr_phys + hhdm_offset;

        self.total_frames = (max_phys_addr / PAGE_SIZE) as usize;
        self.bitmap_size_u64 = (self.total_frames + 63) / 64;
        let bitmap_size_bytes = self.bitmap_size_u64 * 8;

        let mut bitmap_phys_addr = u64::MAX;
        for i in 0..mmap_len {
            let addr = mmap_addr_virt + (i * desc_size);
            let desc = unsafe { &*(addr as *const MemoryDescriptor) };
            if desc.type_ == 7 && desc.phys_start != 0 {
                let region_size = desc.page_count * PAGE_SIZE;
                if region_size >= bitmap_size_bytes as u64 {
                    bitmap_phys_addr = desc.phys_start;
                    break;
                }
            }
        }

        if bitmap_phys_addr == u64::MAX {
            panic!("PMM: Critical - No RAM for Bitmap!");
        }

        self.bitmap_start_addr = bitmap_phys_addr;
        self.bitmap = (bitmap_phys_addr + hhdm_offset) as *mut u64;

        core::ptr::write_bytes(self.bitmap, 0xFF, bitmap_size_bytes);

        for i in 0..mmap_len {
            let addr = mmap_addr_virt + (i * desc_size);
            let desc = unsafe { &*(addr as *const MemoryDescriptor) };
            if desc.type_ == 7 {
                self.mark_region_free(desc.phys_start, desc.page_count as usize);
            }
        }

        let bitmap_pages = (bitmap_size_bytes + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
        self.mark_region_used(bitmap_phys_addr, bitmap_pages);

        self.mark_used(0);
    }

    fn allocate_frame_internal(&mut self) -> Option<u64> {
        unsafe {
            for idx in 0..self.bitmap_size_u64 {
                let entry = *self.bitmap.add(idx);
                if entry != !0 {
                    let inverted = !entry;
                    let bit_idx = inverted.trailing_zeros();
                    let frame_idx = (idx * 64) + bit_idx as usize;

                    if frame_idx >= self.total_frames {
                        return None;
                    }

                    self.mark_used(frame_idx);
                    return Some(frame_idx as u64 * PAGE_SIZE);
                }
            }
        }
        None
    }

    fn free_frame_internal(&mut self, phys_addr: u64) {
        let frame_idx = (phys_addr / PAGE_SIZE) as usize;
        unsafe {
            self.mark_free(frame_idx);
        }
    }

    unsafe fn mark_used(&mut self, frame_idx: usize) {
        let word_idx = frame_idx / 64;
        let bit_idx = frame_idx % 64;
        unsafe { *self.bitmap.add(word_idx) |= 1 << bit_idx };
    }

    unsafe fn mark_free(&mut self, frame_idx: usize) {
        let word_idx = frame_idx / 64;
        let bit_idx = frame_idx % 64;
        unsafe { *self.bitmap.add(word_idx) &= !(1 << bit_idx) };
    }

    fn mark_region_used(&mut self, start_addr: u64, page_count: usize) {
        let start_frame = (start_addr / PAGE_SIZE) as usize;
        for i in 0..page_count {
            unsafe {
                self.mark_used(start_frame + i);
            }
        }
    }

    fn mark_region_free(&mut self, start_addr: u64, page_count: usize) {
        let start_frame = (start_addr / PAGE_SIZE) as usize;
        for i in 0..page_count {
            if start_frame + i < self.total_frames {
                unsafe {
                    self.mark_free(start_frame + i);
                }
            }
        }
    }
}

pub fn init(
    mmap_addr_phys: u64,
    mmap_len: u64,
    desc_size: u64,
    hhdm_offset: u64,
    max_phys_addr: u64,
) {
    serial_println!("[PMM] Init started...");

    unsafe {
        PMM.lock().init_internal(
            mmap_addr_phys,
            mmap_len,
            desc_size,
            hhdm_offset,
            max_phys_addr,
        )
    };

    serial_println!("[PMM] Init finished!");
}

pub fn allocate_frame() -> Option<u64> {
    interrupts::without_interrupts(|| PMM.lock().allocate_frame_internal())
}

pub fn free_frame(phys_addr: u64) {
    interrupts::without_interrupts(|| {
        PMM.lock().free_frame_internal(phys_addr);
    })
}

pub struct KernelFrameAllocator;

unsafe impl FrameAllocator<Size4KiB> for KernelFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        crate::pmm::allocate_frame().map(|phys| PhysFrame::containing_address(PhysAddr::new(phys)))
    }
}
