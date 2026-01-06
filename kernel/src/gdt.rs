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

        // RSP0: Stack pointer used when transitioning from Ring 3 to Ring 0
        // This is CRITICAL for handling interrupts/exceptions from user mode
        tss.privilege_stack_table[0] = {
            const STACK_SIZE: usize = 4096 * 5; // 20 KB
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
            let stack_start = VirtAddr::from_ptr(&raw const STACK);
            stack_start + STACK_SIZE as u64
        };

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
// GDT Layout:
//   Index 0: Null descriptor
//   Index 1: Kernel Code Segment (Ring 0) - Selector 0x08
//   Index 2: Kernel Data Segment (Ring 0) - Selector 0x10
//   Index 3: User Data Segment (Ring 3)   - Selector 0x18 (with RPL 3 = 0x1B)
//   Index 4: User Code Segment (Ring 3)   - Selector 0x20 (with RPL 3 = 0x23)
//   Index 5-6: TSS (takes 2 entries)      - Selector 0x28
lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        // Create a new GDT
        let mut gdt = GlobalDescriptorTable::new();

        // Add a kernel code segment (Ring 0)
        let code_selector = gdt.append(Descriptor::kernel_code_segment());

        // Add a kernel data segment (Ring 0)
        let data_selector = gdt.append(Descriptor::kernel_data_segment());

        // Add user data segment BEFORE user code segment
        // This ordering is required for syscall/sysret compatibility
        let user_data_selector = gdt.append(Descriptor::user_data_segment());

        // Add user code segment
        let user_code_selector = gdt.append(Descriptor::user_code_segment());

        // Add the TSS segment (takes 2 GDT entries in 64-bit mode)
        // We must load the TSS so the CPU knows about our IST
        let tss_selector = gdt.append(Descriptor::tss_segment(&TSS));

        // Return the GDT and the selectors
        (gdt, Selectors {
            code_selector,
            data_selector,
            user_data_selector,
            user_code_selector,
            tss_selector
        })
    };
}

// Helper struct to store segment selectors
pub struct Selectors {
    pub code_selector: SegmentSelector,
    pub data_selector: SegmentSelector,
    pub user_data_selector: SegmentSelector,
    pub user_code_selector: SegmentSelector,
    pub tss_selector: SegmentSelector,
}

/// Get user code segment selector (with RPL 3)
pub fn get_user_code_selector() -> SegmentSelector {
    SegmentSelector::new(4, x86_64::PrivilegeLevel::Ring3)
}

/// Get user data segment selector (with RPL 3)
pub fn get_user_data_selector() -> SegmentSelector {
    SegmentSelector::new(3, x86_64::PrivilegeLevel::Ring3)
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
