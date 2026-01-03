use x86_64::{VirtAddr, structures::paging::OffsetPageTable, structures::paging::PageTable};

/// Khởi tạo một OffsetPageTable từ địa chỉ HHDM.
/// hhdm_offset: địa chỉ mà bạn map RAM vật lý vào (0xffff800000000000)
pub unsafe fn init_mapper(hhdm_offset: u64) -> OffsetPageTable<'static> {
    use x86_64::registers::control::Cr3;

    // 1. Đọc thanh ghi CR3 để biết địa chỉ vật lý của bảng trang PML4 đang dùng
    let (level_4_table_frame, _) = Cr3::read();

    // 2. Chuyển địa chỉ vật lý của PML4 sang địa chỉ ảo (thông qua HHDM)
    let phys = level_4_table_frame.start_address();
    let virt = VirtAddr::new(phys.as_u64() + hhdm_offset);

    // 3. Ép kiểu vùng nhớ đó thành cấu trúc PageTable của Rust
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();
    let l4_table = unsafe { &mut *page_table_ptr };

    // 4. Tạo mapper (OffsetPageTable)
    // Nó sẽ hiểu rằng: mọi truy cập vật lý đều phải cộng thêm hhdm_offset
    unsafe { OffsetPageTable::new(l4_table, VirtAddr::new(hhdm_offset)) }
}
