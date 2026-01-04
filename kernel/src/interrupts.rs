use crate::gdt;
use lazy_static::lazy_static;
use pic8259::ChainedPics;
use shared::helpers::hlt_loop;
use shared::{serial_print, serial_println}; // Chỉ dùng serial_print cho các hàm NGOÀI ngắt
use spin::Mutex;
use x86_64::instructions::interrupts;
use x86_64::instructions::port::Port;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

// --- 1. PIC CONFIGURATION ---
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
        self as usize
    }
}

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
        interrupts::without_interrupts(|| unsafe {
            let mut pics = self.inner.lock();
            pics.initialize();
            // QUAN TRỌNG: Unmask Keyboard (IRQ1) và Timer (IRQ0)
            // 0b1111_1100 (Bit 0=0: Timer ON, Bit 1=0: Keyboard ON, còn lại OFF)
            pics.write_masks(0b1111_1100, 0xFF);
        });
    }

    pub fn notify_end_of_interrupt(&self, interrupt_id: u8) {
        interrupts::without_interrupts(|| unsafe {
            self.inner.lock().notify_end_of_interrupt(interrupt_id);
        });
    }
}

pub static PICS: LockedPics = LockedPics::new(PIC_1_OFFSET, PIC_2_OFFSET);

// --- 2. KEYBOARD RING BUFFER (TĨNH - KHÔNG HEAP) ---
const BUFFER_SIZE: usize = 64;
static mut KEYBOARD_BUFFER: [u8; BUFFER_SIZE] = [0; BUFFER_SIZE];
static mut HEAD: usize = 0;
static mut TAIL: usize = 0;

// Hàm helper để Main Loop lấy dữ liệu an toàn
pub fn pop_scancode() -> Option<u8> {
    unsafe {
        if HEAD == TAIL {
            None
        } else {
            let scancode = KEYBOARD_BUFFER[TAIL];
            TAIL = (TAIL + 1) % BUFFER_SIZE;
            Some(scancode)
        }
    }
}

// --- 3. IDT INITIALIZATION ---
lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();

        idt.breakpoint.set_handler_fn(breakpoint_handler);
        unsafe {
            idt.double_fault.set_handler_fn(double_fault_handler)
                .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
        }
        idt.page_fault.set_handler_fn(page_fault_handler);

        // [MỚI] Thêm handler cho General Protection Fault (Thường gặp nhất)
        idt.general_protection_fault.set_handler_fn(general_protection_fault_handler);

        // [MỚI] Thêm handler cho Stack Segment Fault (Đề phòng lỗi stack)
        idt.stack_segment_fault.set_handler_fn(stack_segment_fault_handler);

        idt[InterruptIndex::Timer.as_u8()].set_handler_fn(timer_interrupt_handler);
        idt[InterruptIndex::Keyboard.as_u8()].set_handler_fn(keyboard_interrupt_handler);

        idt
    };
}

pub fn init_idt() {
    IDT.load();
}

// --- 4. HANDLERS ---

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    serial_println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;
    serial_println!("EXCEPTION: PAGE FAULT");
    serial_println!("Accessed Address: {:?}", Cr2::read());
    serial_println!("Error Code: {:?}", error_code);
    serial_println!("{:#?}", stack_frame);
    hlt_loop();
}

extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    serial_println!("EXCEPTION: GENERAL PROTECTION FAULT");
    serial_println!("Error Code: {:#x}", error_code); // Error code sẽ nói cho ta biết lý do
    serial_println!("{:#?}", stack_frame);
    hlt_loop();
}

extern "x86-interrupt" fn stack_segment_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    serial_println!("EXCEPTION: STACK SEGMENT FAULT");
    serial_println!("Error Code: {:#x}", error_code);
    serial_println!("{:#?}", stack_frame);
    hlt_loop();
}

// Hàm khởi tạo PIT: Thiết lập tần số khoảng 20Hz (20 nhịp/giây)
pub fn init_timer() {
    let mut port_43 = Port::new(0x43); // Cổng điều khiển
    let mut port_40 = Port::new(0x40); // Cổng dữ liệu channel 0

    // Tính toán bộ chia tần số (Divisor)
    // Tần số gốc: 1.193.182 Hz
    // Muốn 20Hz -> Divisor = 1.193.182 / 20 ≈ 59659
    let divisor = 59659u16;

    unsafe {
        // Gửi lệnh: Channel 0, Access lo/hi byte, Mode 3 (Square Wave), Binary
        port_43.write(0x36u8);

        // Gửi Divisor (Low byte trước, High byte sau)
        port_40.write((divisor & 0xFF) as u8);
        port_40.write((divisor >> 8) as u8);
    }
}

// Biến đếm số nhịp (Atomic sẽ tốt hơn, nhưng static mut tạm ổn cho demo)
pub static mut TICKS: u64 = 0;

extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    unsafe {
        TICKS += 1; // Chỉ tăng số, KHÔNG in ấn gì cả
    }
    PICS.notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
}

// --- 5. KEYBOARD HANDLER (SỬA ĐỔI: CỰC KỲ ĐƠN GIẢN) ---
extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    // 1. TUYỆT ĐỐI KHÔNG DÙNG serial_print! Ở ĐÂY NỮA

    // 2. Đọc Port
    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };

    // 3. Ghi vào Buffer tĩnh
    unsafe {
        let next_head = (HEAD + 1) % BUFFER_SIZE;
        if next_head != TAIL {
            KEYBOARD_BUFFER[HEAD] = scancode;
            HEAD = next_head;
        }
        // Nếu đầy thì bỏ qua (Drop), không panic, không print
    }

    // 4. Báo PIC xong
    PICS.notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
}
