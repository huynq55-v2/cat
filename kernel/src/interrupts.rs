use crate::gdt;
use core::sync::atomic::{AtomicU64, Ordering};
use lazy_static::lazy_static;
use pic8259::ChainedPics;
use shared::serial_println;
use spin::Mutex;
use x86_64::instructions::port::Port;
use x86_64::registers::control::Cr2;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

// ============================================================================
// 1. CONSTANTS & GLOBALS
// ============================================================================

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard = PIC_1_OFFSET + 1,
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }
    fn as_usize(self) -> usize {
        self as u8 as usize
    }
}

// Ticks counter (Thread-safe)
pub static TICKS: AtomicU64 = AtomicU64::new(0);

// PICS Driver (Thread-safe wrapper)
pub static PICS: LockedPics = LockedPics::new(PIC_1_OFFSET, PIC_2_OFFSET);

// Keyboard Buffer (Thread-safe)
const BUFFER_SIZE: usize = 64;

struct KeyboardBuffer {
    buffer: [u8; BUFFER_SIZE],
    head: usize,
    tail: usize,
}

lazy_static! {
    static ref KEYBOARD_BUFFER: Mutex<KeyboardBuffer> = Mutex::new(KeyboardBuffer {
        buffer: [0; BUFFER_SIZE],
        head: 0,
        tail: 0,
    });
}

// ============================================================================
// 2. IDT INITIALIZATION (Modern Approach)
// ============================================================================

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        // Exceptions
        idt.breakpoint.set_handler_fn(breakpoint_handler);

        unsafe {
            idt.double_fault
                .set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }

        idt.page_fault.set_handler_fn(page_fault_handler);
        idt.general_protection_fault.set_handler_fn(general_protection_handler);

        // Hardware Interrupts - SỬA: dùng as_usize() thay vì as_u8()
        idt[InterruptIndex::Timer.as_u8()].set_handler_fn(timer_handler);
        idt[InterruptIndex::Keyboard.as_u8()].set_handler_fn(keyboard_handler);

        idt
    };
}

pub fn init_idt() {
    IDT.load();
}

// ============================================================================
// 3. EXCEPTION HANDLERS
// ============================================================================

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    serial_println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    serial_println!("\nPANIC: DOUBLE FAULT EXCEPTION");
    serial_println!("{:#?}", stack_frame);
    loop {
        x86_64::instructions::hlt();
    }
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    let cr2 = Cr2::read();

    serial_println!("EXCEPTION: PAGE FAULT");
    serial_println!("Accessed Address: {:?}", cr2);
    serial_println!("Error Code: {:#x} ({:?})", error_code.bits(), error_code);

    let bits = error_code.bits();

    serial_println!(
        "Cause: {}",
        if bits & PageFaultErrorCode::PROTECTION_VIOLATION.bits() != 0 {
            "Protection violation"
        } else {
            "Page not present"
        }
    );

    serial_println!(
        "Details:
         - Protection violation:  {}
         - Caused by write:       {}
         - User mode fault:       {}
         - Reserved bit set:      {}
         - Instruction fetch:     {}
         - PK (Protection Key):   {}
         - Shadow stack access:   {}
         - HLAT paging:           {}
         - SGX violation:         {} (Intel)
         - RMP violation:         {} (AMD)",
        (bits & PageFaultErrorCode::PROTECTION_VIOLATION.bits()) != 0,
        (bits & PageFaultErrorCode::CAUSED_BY_WRITE.bits()) != 0,
        (bits & PageFaultErrorCode::USER_MODE.bits()) != 0,
        (bits & PageFaultErrorCode::MALFORMED_TABLE.bits()) != 0,
        (bits & PageFaultErrorCode::INSTRUCTION_FETCH.bits()) != 0,
        (bits & PageFaultErrorCode::PROTECTION_KEY.bits()) != 0,
        (bits & PageFaultErrorCode::SHADOW_STACK.bits()) != 0,
        (bits & PageFaultErrorCode::HLAT.bits()) != 0,
        (bits & PageFaultErrorCode::SGX.bits()) != 0,
        (bits & PageFaultErrorCode::RMP.bits()) != 0,
    );

    serial_println!("{:#?}", stack_frame);

    loop {
        x86_64::instructions::hlt();
    }
}

extern "x86-interrupt" fn general_protection_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    serial_println!("EXCEPTION: GENERAL PROTECTION FAULT");
    serial_println!("Error Code: {:#x}", error_code);
    serial_println!("{:#?}", stack_frame);
    loop {
        x86_64::instructions::hlt();
    }
}

// ============================================================================
// 4. HARDWARE INTERRUPT HANDLERS
// ============================================================================

extern "x86-interrupt" fn timer_handler(_stack_frame: InterruptStackFrame) {
    TICKS.fetch_add(1, Ordering::Relaxed);

    unsafe {
        PICS.notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
}

extern "x86-interrupt" fn keyboard_handler(_stack_frame: InterruptStackFrame) {
    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };

    add_scancode(scancode);

    unsafe {
        PICS.notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

// ============================================================================
// 5. HELPER FUNCTIONS
// ============================================================================

fn add_scancode(scancode: u8) {
    let mut lock = KEYBOARD_BUFFER.lock();
    let head = lock.head;
    let next_head = (head + 1) % BUFFER_SIZE;

    // Copy tail ra trước để tránh borrow conflict
    if next_head != lock.tail {
        lock.buffer[head] = scancode;
        lock.head = next_head;
    }
    // Nếu buffer đầy thì drop scancode (silent overflow)
}

pub fn pop_scancode() -> Option<u8> {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut lock = KEYBOARD_BUFFER.lock();
        if lock.head == lock.tail {
            None
        } else {
            let scancode = lock.buffer[lock.tail];
            lock.tail = (lock.tail + 1) % BUFFER_SIZE;
            Some(scancode)
        }
    })
}

pub fn init_timer() {
    const FREQUENCY_HZ: u32 = 1000;
    const PIT_FREQUENCY: u32 = 1_193_182;
    let divisor = PIT_FREQUENCY / FREQUENCY_HZ;

    let mut port_43 = Port::<u8>::new(0x43);
    let mut port_40 = Port::<u8>::new(0x40);

    unsafe {
        port_43.write(0x36);
        port_40.write((divisor & 0xFF) as u8);
        port_40.write((divisor >> 8) as u8);
    }
}

// ============================================================================
// 6. LockedPics Wrapper
// ============================================================================

pub struct LockedPics {
    inner: Mutex<ChainedPics>,
}

impl LockedPics {
    pub const fn new(offset1: u8, offset2: u8) -> Self {
        Self {
            inner: Mutex::new(unsafe { ChainedPics::new(offset1, offset2) }),
        }
    }

    pub fn initialize(&self) {
        // An toàn hơn: disable interrupt khi init PIC
        x86_64::instructions::interrupts::without_interrupts(|| {
            unsafe {
                let mut pics = self.inner.lock();
                pics.initialize();
                // Mở IRQ 0 (timer) và IRQ 1 (keyboard): mask = 0b1111_1100 = 0xFC
                pics.write_masks(0xFC, 0xFF);
            }
        });
    }

    /// Chỉ dùng trong interrupt handler (interrupt đã bị disable tự động)
    pub unsafe fn notify_end_of_interrupt(&self, id: u8) {
        unsafe { self.inner.lock().notify_end_of_interrupt(id) }
    }
}
