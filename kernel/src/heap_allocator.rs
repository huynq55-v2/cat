use linked_list_allocator::LockedHeap;
use x86_64::{
    VirtAddr,
    structures::paging::{
        FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB, mapper::MapToError,
    },
};

use crate::layout::HEAP_SIZE;
use crate::layout::HEAP_START;
use shared::serial_println;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

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

        // In log sau mỗi 100 trang để biết nó không bị kẹt
        if i % 100 == 0 && i != 0 {
            serial_println!("  Mapped {} pages...", i);
        }
    }

    // --- QUAN TRỌNG: CẦN DÒNG NÀY ĐỂ KÍCH HOẠT ALLOCATOR ---
    unsafe {
        ALLOCATOR.lock().init(HEAP_START as *mut u8, HEAP_SIZE);
    }

    serial_println!("Heap initialized successfully!");
    serial_println!("  Start: {:#x}", HEAP_START);
    serial_println!("  End:   {:#x}", HEAP_START + (HEAP_SIZE as u64));

    Ok(())
}
