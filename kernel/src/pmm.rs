use crate::MemoryDescriptor;
use crate::spinlock::Spinlock;
use shared::serial_println;

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
    /// Hàm khởi tạo PMM hoàn chỉnh
    /// - mmap_addr_phys: Địa chỉ vật lý của Memory Map (từ Bootloader)
    /// - hhdm_offset: Offset của vùng Higher Half Direct Map
    /// - max_phys_addr: MaxPhysAddr (từ Bootloader)
    pub unsafe fn init(
        &mut self,
        mmap_addr_phys: u64,
        mmap_len: u64,
        desc_size: u64,
        hhdm_offset: u64,
        max_phys_addr: u64,
    ) {
        serial_println!("[PMM] Init started...");

        let mmap_addr_virt = mmap_addr_phys + hhdm_offset;

        // 1. Tính toán kích thước Bitmap
        self.total_frames = (max_phys_addr / PAGE_SIZE) as usize;
        self.bitmap_size_u64 = (self.total_frames + 63) / 64;
        let bitmap_size_bytes = self.bitmap_size_u64 * 8;

        serial_println!(
            "[PMM] Total frames: {}, Bitmap size: {} bytes",
            self.total_frames,
            bitmap_size_bytes
        );

        // 2. Tìm vùng nhớ để đặt Bitmap
        // Khởi tạo bằng giá trị bất hợp lý để phân biệt với địa chỉ 0
        let mut bitmap_phys_addr = u64::MAX;

        shared::serial_println!("[PMM] Scanning {} regions for bitmap space...", mmap_len);

        for i in 0..mmap_len {
            let addr = mmap_addr_virt + (i * desc_size);
            let desc = unsafe { &*(addr as *const MemoryDescriptor) };

            // Chỉ tìm Conventional và BỎ QUA địa chỉ 0 (để tránh lỗi logic và an toàn hơn)
            if desc.type_ == 7 && desc.phys_start != 0 {
                let region_size = desc.page_count * PAGE_SIZE;

                // Debug log gọn hơn
                // shared::serial_println!("[DEBUG] Candidate: {:#x}, Size: {}", desc.phys_start, region_size);

                if region_size >= bitmap_size_bytes as u64 {
                    bitmap_phys_addr = desc.phys_start;
                    shared::serial_println!(
                        "[PMM] Found suitable region at {:#x}",
                        bitmap_phys_addr
                    );
                    break;
                }
            }
        }

        // Kiểm tra nếu không tìm thấy (vẫn là giá trị MAX)
        if bitmap_phys_addr == u64::MAX {
            panic!("PMM: Critical - No RAM for Bitmap!");
        }

        serial_println!("[PMM] Placing Bitmap at phys: {:#x}", bitmap_phys_addr);

        self.bitmap_start_addr = bitmap_phys_addr;
        self.bitmap = (bitmap_phys_addr + hhdm_offset) as *mut u64;

        // --- CHIẾN THUẬT MỚI: WHITELIST ---

        // 3. FILL TOÀN BỘ BITMAP LÀ 0xFF (USED/BUSY)
        // Mặc định coi như toàn bộ RAM là "Không dùng được" hoặc "Không tồn tại"
        serial_println!("[PMM] Filling bitmap with 0xFF (Mark All Used)...");
        unsafe { core::ptr::write_bytes(self.bitmap, 0xFF, bitmap_size_bytes) };

        // 4. CHỈ KHAI BÁO CÁC VÙNG CONVENTIONAL LÀ FREE
        serial_println!("[PMM] Scanning mmap to free Conventional Memory...");
        for i in 0..mmap_len {
            let addr = mmap_addr_virt + (i * desc_size);
            let desc = unsafe { &*(addr as *const MemoryDescriptor) };

            if desc.type_ == 7 {
                // Chỉ quan tâm Conventional
                self.mark_region_free(desc.phys_start, desc.page_count as usize);
            }
        }

        // 5. Đánh dấu USED cho chính vùng nhớ chứa Bitmap
        // (Vì bước 4 đã lỡ tay free nó rồi, giờ phải lock lại)
        let bitmap_pages = (bitmap_size_bytes + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;
        self.mark_region_used(bitmap_phys_addr, bitmap_pages);

        // 6. Đánh dấu USED cho Frame 0
        unsafe { self.mark_used(0) };

        // Không cần bước xử lý Padding nữa vì ta đã fill 0xFF từ đầu rồi!

        serial_println!("[PMM] Init finished successfully!");
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

    // --- HELPERS ---

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
            // Kiểm tra bounds check để tránh crash nếu UEFI báo láo
            if start_frame + i < self.total_frames {
                unsafe {
                    self.mark_free(start_frame + i);
                }
            }
        }
    }
}

// --- PUBLIC API ---
// Đây là các hàm wrapper để bên ngoài gọi dễ dàng mà không cần quan tâm Spinlock

pub fn init(
    mmap_addr_phys: u64,
    mmap_len: u64,
    desc_size: u64,
    hhdm_offset: u64,
    max_phys_addr: u64,
) {
    // Lock PMM và gọi hàm init bên trong
    unsafe {
        PMM.lock().init(
            mmap_addr_phys,
            mmap_len,
            desc_size,
            hhdm_offset,
            max_phys_addr,
        )
    };
}

pub fn allocate_frame() -> Option<u64> {
    PMM.lock().allocate_frame_internal()
}

pub fn free_frame(phys_addr: u64) {
    PMM.lock().free_frame_internal(phys_addr);
}
