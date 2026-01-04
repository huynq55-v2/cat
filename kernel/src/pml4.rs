use x86_64::{VirtAddr, structures::paging::OffsetPageTable, structures::paging::PageTable};

pub unsafe fn init_mapper(hhdm_offset: u64) -> OffsetPageTable<'static> {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = VirtAddr::new(phys.as_u64() + hhdm_offset);

    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();
    let l4_table = unsafe { &mut *page_table_ptr };

    unsafe { OffsetPageTable::new(l4_table, VirtAddr::new(hhdm_offset)) }
}
