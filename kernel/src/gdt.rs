use lazy_static::lazy_static;
use x86_64::VirtAddr;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;

// 1. Định nghĩa chỉ số cho IST (Interrupt Stack Table)
// Chúng ta sẽ dùng index 0 cho Double Fault
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

// 2. Tạo TSS (Task State Segment)
lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();

        // Cấp phát một vùng nhớ làm Stack riêng cho Double Fault
        // Kích thước stack: 20KB (4096 * 5)
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 5;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            // Vì STACK là static mut, truy cập nó là unsafe
            // Nhưng vì ta dùng lazy_static nên nó chỉ chạy 1 lần lúc init -> An toàn
            let stack_start = VirtAddr::from_ptr(&raw const STACK);
            let stack_end = stack_start + STACK_SIZE as u64;

            // Stack trong x86 mọc từ cao xuống thấp, nên trả về địa chỉ cuối (đỉnh stack)
            stack_end
        };
        tss
    };
}

// 3. Tạo GDT (Global Descriptor Table)
lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();

        // Thêm các Segment chuẩn của Kernel
        let code_selector = gdt.append(Descriptor::kernel_code_segment());
        let tss_selector = gdt.append(Descriptor::tss_segment(&TSS));

        // (Kernel Data Segment được thêm mặc định hoặc không cần thiết trong 64-bit nếu không dùng FS/GS)

        (gdt, Selectors { code_selector, tss_selector })
    };
}

// Struct helper để lưu các Selector (cần dùng để load vào thanh ghi CS, TR)
struct Selectors {
    code_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

// 4. Hàm Init (Public) để gọi từ main.rs
pub fn init() {
    use x86_64::instructions::segmentation::{CS, Segment};
    use x86_64::instructions::tables::load_tss;

    // Load GDT vào CPU
    GDT.0.load();

    unsafe {
        // Nạp lại Code Segment (CS)
        // Bắt buộc để CPU biết chúng ta đang chạy với GDT mới
        CS::set_reg(GDT.1.code_selector);

        // Load TSS (Task Register)
        load_tss(GDT.1.tss_selector);
    }
}
