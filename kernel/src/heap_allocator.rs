use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;
use linked_list_allocator::Heap;
use spin::Mutex;
use x86_64::{
    VirtAddr,
    instructions::interrupts, // Import module interrupts
    structures::paging::{
        FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB, mapper::MapToError,
    },
};

use crate::layout::HEAP_SIZE;
use crate::layout::HEAP_START;
use shared::serial_println;

// --- 1. TẠO WRAPPER AN TOÀN VỚI NGẮT ---
// Thay vì dùng LockedHeap có sẵn, ta tự định nghĩa để kiểm soát việc lock
pub struct SafeLockedHeap(Mutex<Heap>);

impl SafeLockedHeap {
    /// Tạo một heap rỗng
    pub const fn empty() -> Self {
        Self(Mutex::new(Heap::empty()))
    }

    /// Khởi tạo vùng nhớ cho heap
    pub unsafe fn init(&self, start_addr: usize, size: usize) {
        // Bắt buộc tắt ngắt khi init để an toàn (dù thường init chạy lúc chưa có ngắt)
        interrupts::without_interrupts(|| unsafe {
            self.0.lock().init(start_addr as *mut u8, size);
        });
    }
}

// --- 2. IMPLEMENT GLOBAL ALLOCATOR ---
// Đây là nơi phép màu xảy ra: Mỗi lần alloc/dealloc đều tắt ngắt trước!
unsafe impl GlobalAlloc for SafeLockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        interrupts::without_interrupts(|| {
            self.0
                .lock()
                .allocate_first_fit(layout)
                .ok()
                .map(|ptr| ptr.as_ptr())
                .unwrap_or(null_mut())
        })
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        interrupts::without_interrupts(|| {
            use core::ptr::NonNull;
            if let Some(ptr) = NonNull::new(ptr) {
                unsafe { self.0.lock().deallocate(ptr, layout) };
            }
        })
    }
}

// Đăng ký Allocator của chúng ta
#[global_allocator]
static ALLOCATOR: SafeLockedHeap = SafeLockedHeap::empty();

// --- 3. HÀM INIT CŨ (Cập nhật gọi hàm init mới) ---
pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE as u64 - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    serial_println!(
        "Initializing Heap at {:#x} (Size: {} Bytes)",
        HEAP_START,
        HEAP_SIZE
    );

    for (i, page) in page_range.enumerate() {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        unsafe {
            mapper.map_to(page, frame, flags, frame_allocator)?.flush();
        }
    }

    // GỌI HÀM INIT CỦA WRAPPER
    unsafe {
        ALLOCATOR.init(HEAP_START as usize, HEAP_SIZE);
    }

    serial_println!("Heap initialized successfully with Interrupt Safety!");
    Ok(())
}
