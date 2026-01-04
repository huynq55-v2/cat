// Import code modules
use crate::gdt;
use lazy_static::lazy_static;
use pic8259::ChainedPics;
use shared::helpers::hlt_loop;
use shared::serial_println;
use spin::Mutex;
use x86_64::instructions::interrupts;
use x86_64::instructions::port::Port;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

// Offsets for the PICs (Programmable Interrupt Controllers)
// The CPU uses interrupts 0-31 for exceptions (like Page Fault).
// So we must remap the PIC interrupts to start at 32 to avoid conflicts.
pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

// Enum to define interrupt indices for easier usage
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,        // IRQ 0 -> Timer
    Keyboard = PIC_1_OFFSET + 1, // IRQ 1 -> Keyboard
}

impl InterruptIndex {
    // Helper to get the index as u8
    fn as_u8(self) -> u8 {
        self as u8
    }
}

// Wrapper for ChainedPics that allows thread-safe access
pub struct LockedPics {
    inner: Mutex<ChainedPics>,
}

impl LockedPics {
    // Constructor
    pub const fn new(offset1: u8, offset2: u8) -> Self {
        Self {
            inner: Mutex::new(unsafe { ChainedPics::new(offset1, offset2) }),
        }
    }

    // Initialize the PICs
    pub fn initialize(&self) {
        interrupts::without_interrupts(|| unsafe {
            let mut pics = self.inner.lock();
            pics.initialize();
            // Mask all interrupts except Timer and Keyboard for now
            // 0b1111_1100 means the last two bits (0 and 1) are 0 (enabled)
            pics.write_masks(0b1111_1100, 0xFF);
        });
    }

    // Send End of Interrupt (EOI) signal to the PICs
    // This tells the PIC that we are done handling the current interrupt
    // and it can send the next one.
    pub fn notify_end_of_interrupt(&self, interrupt_id: u8) {
        interrupts::without_interrupts(|| unsafe {
            self.inner.lock().notify_end_of_interrupt(interrupt_id);
        });
    }
}

// Global static instance of the PICS protection by a Mutex
pub static PICS: LockedPics = LockedPics::new(PIC_1_OFFSET, PIC_2_OFFSET);

// Simple Keyboard Buffer (Ring Buffer)
const BUFFER_SIZE: usize = 64;
static mut KEYBOARD_BUFFER: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
// Head index (where we write new data)
static mut HEAD: usize = 0;
// Tail index (where we read data from)
static mut TAIL: usize = 0;

// Function to pop a scancode from the keyboard buffer
pub fn pop_scancode() -> Option<u8> {
    unsafe {
        // If pointers are equal, buffer is empty
        if HEAD == TAIL {
            None
        } else {
            // Read value at tail
            let scancode = KEYBOARD_BUFFER[TAIL];
            // Advance tail pointer (wrap around if needed)
            TAIL = (TAIL + 1) % BUFFER_SIZE;
            Some(scancode)
        }
    }
}

// Initialize the IDT (Interrupt Descriptor Table)
lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        // Set handler for Breakpoint exception
        idt.breakpoint.set_handler_fn(breakpoint_handler);

        // Set handler for Double Fault exception
        unsafe {
            idt.double_fault.set_handler_fn(double_fault_handler)
                // Use a separate stack for double faults to avoid stack overflow loops
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }

        // Set handler for Page Fault exception
        idt.page_fault.set_handler_fn(page_fault_handler);

        // Set handler for General Protection Fault
        idt.general_protection_fault.set_handler_fn(general_protection_fault_handler);

        // Set handler for Stack Segment Fault
        idt.stack_segment_fault.set_handler_fn(stack_segment_fault_handler);

        // Set handlers for hardware interrupts (Timer and Keyboard)
        idt[InterruptIndex::Timer.as_u8()].set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_u8()].set_handler_fn(keyboard_interrupt_handler);

        idt
    };
}

// Public function to load the IDT
pub fn init_idt() {
    IDT.load();
}

// Handler for Breakpoint Exception
extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    serial_println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

// Handler for Double Fault Exception
// This is critical because if this handler fails, the CPU triple faults (resets).
extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

// Handler for Page Fault Exception
// Occurs when accessing an invalid memory address
extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;
    serial_println!("EXCEPTION: PAGE FAULT");
    // CR2 register contains the virtual address that caused the fault
    serial_println!("Accessed Address: {:?}", Cr2::read());
    serial_println!("Error Code: {:?}", error_code);
    serial_println!("{:#?}", stack_frame);
    // Halt the system as we cannot recover
    hlt_loop();
}

// Handler for General Protection Fault (GPF)
extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    serial_println!("EXCEPTION: GENERAL PROTECTION FAULT");
    serial_println!("Error Code: {:#x}", error_code);
    serial_println!("{:#?}", stack_frame);
    hlt_loop();
}

// Handler for Stack Segment Fault
extern "x86-interrupt" fn stack_segment_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    serial_println!("EXCEPTION: STACK SEGMENT FAULT");
    serial_println!("Error Code: {:#x}", error_code);
    serial_println!("{:#?}", stack_frame);
    hlt_loop();
}

// Initialize the PIT (Programmable Interval Timer)
pub fn init_timer() {
    // Port 0x43 is the Mode/Command register
    let mut port_43 = Port::new(0x43);
    // Port 0x40 is Channel 0 data
    let mut port_40 = Port::new(0x40);

    // Divisor for 20Hz frequency
    // 1193182 Hz / 59659 ≈ 20 Hz
    let divisor = 59659u16;

    unsafe {
        // Send command byte:
        // 0x36 = 00 11 011 0
        // Channel 0 | Access lo/hi byte | Mode 3 (Square Wave) | Binary mode
        port_43.write(0x36u8);

        // Send low_byte of divisor
        port_40.write((divisor & 0xFF) as u8);
        // Send high_byte of divisor
        port_40.write((divisor >> 8) as u8);
    }
}

// Tick counter
pub static mut TICKS: u64 = 0;

// Timer Interrupt Handler (IRQ 0)
extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        TICKS += 1;
    }
    // Acknowledge the interrupt so PIC sends the next one
    PICS.notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
}

// Keyboard Interrupt Handler (IRQ 1)
extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // Read the scancode from the keyboard data port (0x60)
    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };

    unsafe {
        // Calculate next head position
        let next_head = (HEAD + 1) % BUFFER_SIZE;
        // Check if buffer is full (if next_head == TAIL)
        if next_head != TAIL {
            // Write to buffer
            KEYBOARD_BUFFER[HEAD] = scancode;
            // Update HEAD
            HEAD = next_head;
        }
    }

    // Acknowledge the interrupt
    PICS.notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
}
