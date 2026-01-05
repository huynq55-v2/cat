// Import necessary modules
use spin::Mutex;
use x86_64::PhysAddr;
use x86_64::instructions::interrupts;
use x86_64::structures::paging::{FrameAllocator, PhysFrame, Size4KiB};

// Page size is 4KB
pub const PAGE_SIZE: u64 = 4096;

// UEFI Memory Descriptor structure
// Matches the UEFI specification for EFI_MEMORY_DESCRIPTOR
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct MemoryDescriptor {
    pub type_: u32,      // Memory type
    pub pad: u32,        // Padding
    pub phys_start: u64, // Physical start address
    pub virt_start: u64, // Virtual start address
    pub page_count: u64, // Number of 4KiB pages
    pub attribute: u64,  // Attributes (permissions, cacheability)
}

unsafe impl Send for BitmapPmm {}

// Bitmap Physical Memory Manager
// Uses a bitmap to track free/used frames
struct BitmapPmm {
    // Pointer to the start of the bitmap logic
    bitmap: *mut u64,
    // Total number of physical frames managed
    total_frames: usize,
    // Size of the bitmap in u64 words
    bitmap_size_u64: usize,
    // Physical address where the bitmap is stored
    bitmap_start_addr: u64,
}

// Global PMM instance protected by a Mutex
static PMM: Mutex<BitmapPmm> = Mutex::new(BitmapPmm {
    bitmap: core::ptr::null_mut(),
    total_frames: 0,
    bitmap_size_u64: 0,
    bitmap_start_addr: 0,
});

impl BitmapPmm {
    // Internal initialization function
    unsafe fn init_internal(
        &mut self,
        mmap_addr_phys: u64,
        mmap_len: u64,
        desc_size: u64,
        hhdm_offset: u64,
        max_phys_addr: u64,
    ) {
        unsafe {
            // Calculate virtual address of memory map
            let mmap_addr_virt = mmap_addr_phys + hhdm_offset;

            // Calculate total frames needed to cover max physical address
            self.total_frames = (max_phys_addr / PAGE_SIZE) as usize;
            // Calculate bitmap size in u64 words (64 bits per word)
            self.bitmap_size_u64 = self.total_frames.div_ceil(64);
            let bitmap_size_bytes = self.bitmap_size_u64 * 8;

            // Find a large enough free region to store the bitmap itself for us
            let mut bitmap_phys_addr = u64::MAX;
            for i in 0..mmap_len {
                let addr = mmap_addr_virt + (i * desc_size);
                let desc = &*(addr as *const MemoryDescriptor);
                // Type 7 is Conventional Memory (Usable RAM)
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
            // Map the bitmap pointer to the higher half
            self.bitmap = (bitmap_phys_addr + hhdm_offset) as *mut u64;

            // Initialize bitmap to all 1s (all used) initially
            core::ptr::write_bytes(self.bitmap, 0xFF, bitmap_size_bytes);

            // Iterate memory map again and mark usable regions as free (0)
            for i in 0..mmap_len {
                let addr = mmap_addr_virt + (i * desc_size);
                let desc = &*(addr as *const MemoryDescriptor);
                if desc.type_ == 7 {
                    self.mark_region_free(desc.phys_start, desc.page_count as usize);
                }
            }

            // Mark the memory occupied by the bitmap itself as used
            let bitmap_pages = bitmap_size_bytes.div_ceil(PAGE_SIZE as usize);
            self.mark_region_used(bitmap_phys_addr, bitmap_pages);

            // Mark frame 0 as used (null pointer protection)
            self.mark_used(0);
        }
    }

    // Allocation Logic: Find first 0 bit in bitmap
    fn allocate_frame_internal(&mut self) -> Option<u64> {
        unsafe {
            for idx in 0..self.bitmap_size_u64 {
                let entry = *self.bitmap.add(idx);
                if entry != !0 {
                    // If not all bits are 1
                    // Find first zero bit (inverted entry trailing zeros)
                    let inverted = !entry;
                    let bit_idx = inverted.trailing_zeros();
                    let frame_idx = (idx * 64) + bit_idx as usize;

                    if frame_idx >= self.total_frames {
                        return None; // Out of bounds
                    }

                    // Mark as used
                    self.mark_used(frame_idx);
                    // Return physical address
                    return Some(frame_idx as u64 * PAGE_SIZE);
                }
            }
        }
        None // OOM
    }

    // Helper to mark a specific frame as used (set bit to 1)
    unsafe fn mark_used(&mut self, frame_idx: usize) {
        let word_idx = frame_idx / 64;
        let bit_idx = frame_idx % 64;
        unsafe { *self.bitmap.add(word_idx) |= 1 << bit_idx };
    }

    // Helper to mark a specific frame as free (set bit to 0)
    unsafe fn mark_free(&mut self, frame_idx: usize) {
        let word_idx = frame_idx / 64;
        let bit_idx = frame_idx % 64;
        unsafe { *self.bitmap.add(word_idx) &= !(1 << bit_idx) };
    }

    // Helper to mark a range of frames as used
    fn mark_region_used(&mut self, start_addr: u64, page_count: usize) {
        let start_frame = (start_addr / PAGE_SIZE) as usize;
        for i in 0..page_count {
            unsafe {
                self.mark_used(start_frame + i);
            }
        }
    }

    // Helper to mark a range of frames as free
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

// Public initialization function called by main
pub fn init(
    mmap_addr_phys: u64,
    mmap_len: u64,
    desc_size: u64,
    hhdm_offset: u64,
    max_phys_addr: u64,
) {
    println!("[PMM] Init started...");

    unsafe {
        PMM.lock().init_internal(
            mmap_addr_phys,
            mmap_len,
            desc_size,
            hhdm_offset,
            max_phys_addr,
        )
    };

    println!("[PMM] Init finished!");
}

// Public allocation function
pub fn allocate_frame() -> Option<u64> {
    interrupts::without_interrupts(|| PMM.lock().allocate_frame_internal())
}

// Implement the FrameAllocator trait from x86_64 crate
pub struct KernelFrameAllocator;

unsafe impl FrameAllocator<Size4KiB> for KernelFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        crate::pmm::allocate_frame().map(|phys| PhysFrame::containing_address(PhysAddr::new(phys)))
    }
}
