// Import necessary modules
use core::alloc::{GlobalAlloc, Layout};

use linked_list_allocator::Heap;
use spin::Mutex;
use x86_64::{
    VirtAddr,
    instructions::interrupts,
    structures::paging::{
        FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB, mapper::MapToError,
    },
};

// Define the virtual address where the heap starts
// We place it in the higher half memory to keep it separate from user space
pub const KERNEL_HEAP_START: u64 = 0xFFFF_9000_0000_0000;

// Define the size of the heap (100 KB)
pub const KERNEL_HEAP_SIZE: usize = 100 * 1024;

// Wrapper around the allocator to make it thread-safe using a spinlock
pub struct SafeLockedHeap(Mutex<Heap>);

impl SafeLockedHeap {
    // Create an empty heap
    pub const fn empty() -> Self {
        Self(Mutex::new(Heap::empty()))
    }

    // Initialize the heap with a given range of memory
    pub unsafe fn init(&self, start_addr: usize, size: usize) {
        // Disable interrupts during initialization to prevent potential deadlocks relative to allocs ?
        // (Typically init is done at boot up so interrupts might be disabled anyway, but good safety)
        interrupts::without_interrupts(|| unsafe {
            self.0.lock().init(start_addr as *mut u8, size);
        });
    }
}

// Implement the GlobalAlloc trait required by Rust for memory allocation
unsafe impl GlobalAlloc for SafeLockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Disable interrupts to ensure thread safety
        interrupts::without_interrupts(|| {
            self.0
                .lock()
                .allocate_first_fit(layout)
                .ok()
                .map(|ptr| ptr.as_ptr())
                .unwrap_or_default() // Return null pointer on failure
        })
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        interrupts::without_interrupts(|| {
            use core::ptr::NonNull;
            // Ensure pointer is not null before deallocating
            if let Some(ptr) = NonNull::new(ptr) {
                unsafe { self.0.lock().deallocate(ptr, layout) };
            }
        })
    }
}

// Define the global allocator static variable
#[global_allocator]
static ALLOCATOR: SafeLockedHeap = SafeLockedHeap::empty();

// Function to initialize the heap
// This involves mapping the pages for the heap and then initializing the allocator
pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    // Calculate the range of pages that the heap will cover
    let page_range = {
        let heap_start = VirtAddr::new(KERNEL_HEAP_START);
        let heap_end = heap_start + KERNEL_HEAP_SIZE as u64 - 1u64;
        let heap_start_page = Page::containing_address(heap_start);
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page)
    };

    println!(
        "Initializing Heap at {:#x} (Size: {} Bytes)",
        KERNEL_HEAP_START, KERNEL_HEAP_SIZE
    );

    // Map all pages in the heap range
    for page in page_range {
        // Allocate a physical frame for the page
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        // Flags: Present (valid) and Writable
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        unsafe {
            // Create the mapping and flush the TLB
            mapper.map_to(page, frame, flags, frame_allocator)?.flush();
        }
    }

    // Initialize the allocator with the mapped memory
    unsafe {
        ALLOCATOR.init(KERNEL_HEAP_START as usize, KERNEL_HEAP_SIZE);
    }

    println!("Heap initialized successfully with Interrupt Safety!");
    Ok(())
}
