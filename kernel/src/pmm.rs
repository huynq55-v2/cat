use crate::MemoryDescriptor;
use crate::spinlock::Spinlock;

pub const PAGE_SIZE: u64 = 4096;

// Khẳng định với Rust rằng BitmapPmm có thể chuyển giao giữa các luồng một cách an toàn.
// Chúng ta được phép làm điều này vì quyền truy cập đã được bảo vệ bởi Spinlock.
unsafe impl Send for BitmapPmm {}

pub struct BitmapPmm {
    bitmap: *mut u64,
    total_frames: usize,
    bitmap_size_u64: usize,
    bitmap_start_addr: u64,
}

// Thay static mut bằng static với Spinlock
// Khởi tạo const an toàn
pub static PMM: Spinlock<BitmapPmm> = Spinlock::new(BitmapPmm {
    bitmap: core::ptr::null_mut(),
    total_frames: 0,
    bitmap_size_u64: 0,
    bitmap_start_addr: 0,
});

impl BitmapPmm {
    // Hàm này bây giờ là method của instance (bên trong lock), không phải static function rời rạc
    // Ta đổi tên thành 'init_internal' hoặc để logic init ra ngoài wrapper
    pub unsafe fn init(
        &mut self,
        mmap_addr_phys: u64,
        mmap_len: u64,
        desc_size: u64,
        hhdm_offset: u64,
    ) {
        let mmap_addr_virt = mmap_addr_phys + hhdm_offset;
        let mut max_phys_addr = 0;

        // 1. Tính toán kích thước RAM
        for i in 0..mmap_len {
            let addr = mmap_addr_virt + (i * desc_size);
            let desc = unsafe { &*(addr as *const MemoryDescriptor) };
            let end = desc.phys_start + (desc.page_count * PAGE_SIZE);
            if end > max_phys_addr {
                max_phys_addr = end;
            }
        }

        let total_frames = (max_phys_addr / PAGE_SIZE) as usize;
        let bitmap_size_u64 = (total_frames + 63) / 64;
        let bitmap_size_bytes = bitmap_size_u64 * 8;

        // 2. Tìm chỗ đặt Bitmap
        let mut bitmap_phys_addr: u64 = 0;
        let mut found = false;

        for i in 0..mmap_len {
            let addr = mmap_addr_virt + (i * desc_size);
            let desc = unsafe { &*(addr as *const MemoryDescriptor) };

            if desc.type_ == 7 {
                let region_size = desc.page_count * PAGE_SIZE;
                if region_size > (bitmap_size_bytes as u64) {
                    bitmap_phys_addr = desc.phys_start;
                    found = true;
                    break;
                }
            }
        }

        if !found {
            panic!("Not enough RAM for Bitmap!");
        }

        // 3. Update dữ liệu vào self (đang được lock)
        self.bitmap = (bitmap_phys_addr + hhdm_offset) as *mut u64;
        self.total_frames = total_frames;
        self.bitmap_size_u64 = bitmap_size_u64;
        self.bitmap_start_addr = bitmap_phys_addr;

        // 4. Init Bitmap: Used (All 1s)
        unsafe { core::ptr::write_bytes(self.bitmap, 0xFF, bitmap_size_u64 * 8) };

        // 5. Free Conventional Regions
        for i in 0..mmap_len {
            let addr = mmap_addr_virt + (i * desc_size);
            let desc = unsafe { &*(addr as *const MemoryDescriptor) };
            if desc.type_ == 7 {
                self.free_region(desc.phys_start, desc.page_count as usize);
            }
        }

        // 6. Mark Bitmap & Null as Used
        let bitmap_pages = (bitmap_size_bytes + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
        self.mark_region_used(bitmap_phys_addr, bitmap_pages);
        unsafe { self.mark_used(0) };

        shared::serial_println!("PMM Initialized (Thread-Safe).");
    }

    // Logic tìm trang trống (Internal)
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

    // Logic giải phóng (Internal)
    fn free_frame_internal(&mut self, phys_addr: u64) {
        let frame_idx = (phys_addr / PAGE_SIZE) as usize;
        unsafe {
            self.mark_free(frame_idx);
        }
    }

    // --- Helpers (Private) ---
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

    fn free_region(&mut self, start_addr: u64, page_count: usize) {
        let start_frame = (start_addr / PAGE_SIZE) as usize;
        for i in 0..page_count {
            unsafe {
                self.mark_free(start_frame + i);
            }
        }
    }

    fn mark_region_used(&mut self, start_addr: u64, page_count: usize) {
        let start_frame = (start_addr / PAGE_SIZE) as usize;
        for i in 0..page_count {
            unsafe {
                self.mark_used(start_frame + i);
            }
        }
    }
}

// --- PUBLIC API ---
// Đây là các hàm wrapper để bên ngoài gọi dễ dàng mà không cần quan tâm Spinlock

pub fn init(mmap_addr_phys: u64, mmap_len: u64, desc_size: u64, hhdm_offset: u64) {
    // Lock PMM và gọi hàm init bên trong
    unsafe {
        PMM.lock()
            .init(mmap_addr_phys, mmap_len, desc_size, hhdm_offset)
    };
}

pub fn allocate_frame() -> Option<u64> {
    PMM.lock().allocate_frame_internal()
}

pub fn free_frame(phys_addr: u64) {
    PMM.lock().free_frame_internal(phys_addr);
}
