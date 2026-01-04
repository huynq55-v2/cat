// Import necessary modules
use lazy_static::lazy_static;
use x86_64::VirtAddr;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;

// Define the index for the Double Fault stack in the IST
// We use index 0
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

// Initialize the TSS (Task State Segment) lazily
lazy_static! {
    static ref TSS: TaskStateSegment = {
        // Create a new TSS
        let mut tss = TaskStateSegment::new();

        // Define the stack for double faults in the Interrupt Stack Table (IST)
        // This ensures that when a double fault occurs, the CPU switches to a fresh stack.
        // This prevents a "triple fault" (system reset) if the main stack overflows.
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            // Define the stack size (5 pages)
            const STACK_SIZE: usize = 4096 * 5;
            // Create a static array for the stack
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            // Get the virtual address of the stack top
            // &raw const is used to get a raw pointer safely without creating a reference to mutable static
            let stack_start = VirtAddr::from_ptr(&raw const STACK);

            // Return the top address (stacks grow downwards)
            stack_start + STACK_SIZE as u64
        };
        tss
    };
}

// Initialize the GDT (Global Descriptor Table) lazily
lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        // Create a new GDT
        let mut gdt = GlobalDescriptorTable::new();

        // Add a kernel code segment
        let code_selector = gdt.append(Descriptor::kernel_code_segment());

        // Add a kernel data segment
        let data_selector = gdt.append(Descriptor::kernel_data_segment());

        // Add the TSS segment
        // We must load the TSS so the CPU knows about our IST
        let tss_selector = gdt.append(Descriptor::tss_segment(&TSS));

        // Return the GDT and the selectors
        (gdt, Selectors { code_selector, data_selector, tss_selector })
    };
}

// Helper struct to store segment selectors
struct Selectors {
    code_selector: SegmentSelector,
    data_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

// Function to initialize the GDT
pub fn init() {
    // Import segment register instructions
    use x86_64::instructions::segmentation::{CS, DS, ES, SS, Segment};
    use x86_64::instructions::tables::load_tss;

    // Load the GDT into the CPU
    GDT.0.load();

    // Reload segment registers
    unsafe {
        // Set the Code Segment register (CS)
        // This effectively switches to our new GDT
        CS::set_reg(GDT.1.code_selector);

        // Set Data Segment registers (SS, DS, ES)
        // In 64-bit mode these are mostly ignored but good practice to set
        SS::set_reg(GDT.1.data_selector);
        DS::set_reg(GDT.1.data_selector);
        ES::set_reg(GDT.1.data_selector);

        // Load the Task State Segment (TSS)
        // This allows the CPU to find the interrupt stacks
        load_tss(GDT.1.tss_selector);
    }
}
