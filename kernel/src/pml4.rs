// Import necessary types for paging
use x86_64::{VirtAddr, structures::paging::OffsetPageTable, structures::paging::PageTable};

// Function to initialize the memory mapper
// This uses "Offset Page Table" (also known as Higher Half Direct Mapping or HHDM)
// This technique maps all physical memory to a virtual address range starting at 'hhdm_offset'
pub unsafe fn init_mapper(hhdm_offset: u64) -> OffsetPageTable<'static> {
    // Import CR3 register (holds the physical address of the level 4 page table)
    use x86_64::registers::control::Cr3;

    // Read the current level 4 page table frame from CR3
    let (level_4_table_frame, _) = Cr3::read();

    // Get the physical address of the page table
    let phys = level_4_table_frame.start_address();
    // Calculate the virtual address where we can access this page table
    // by adding the HHDM offset to the physical address
    let virt = VirtAddr::new(phys.as_u64() + hhdm_offset);

    // Create a mutable pointer to the page table
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    // Convert the raw pointer to a mutable reference
    let l4_table = unsafe { &mut *page_table_ptr };

    // Create and return the OffsetPageTable mapper
    // This mapper allows us to map/unmap pages by assuming that
    // phys_addr + offset = virt_addr for the entire physical memory
    unsafe { OffsetPageTable::new(l4_table, VirtAddr::new(hhdm_offset)) }
}
