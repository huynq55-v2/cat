use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::SerialPort;
use x86_64::instructions::interrupts;

// Khởi tạo Serial Port với spin::Mutex
lazy_static! {
    pub static ref SERIAL1: Mutex<SerialPort> = {
        let mut serial_port = unsafe { SerialPort::new(0x3F8) };
        serial_port.init();
        Mutex::new(serial_port)
    };
}

// Hàm in nội bộ (ẩn)
#[doc(hidden)]
pub fn _print(args: ::core::fmt::Arguments) {
    use core::fmt::Write;

    // --- KHÓA AN TOÀN (Interrupt-Safe Lock) ---
    // 1. Lưu trạng thái & Tắt ngắt
    // 2. Chạy closure bên trong
    // 3. Khôi phục trạng thái ngắt
    interrupts::without_interrupts(|| {
        // 4. Lấy khóa (Spin) -> An toàn vì không ai ngắt được ta lúc này
        let mut serial = SERIAL1.lock();

        // 5. Làm việc (In ra)
        serial.write_fmt(args).expect("Printing to serial failed");

        // 6. Nhả khóa (Tự động khi ra khỏi closure này)
    });
}

// --- HÀM MỚI: In an toàn khi Panic ---
pub fn print_panic(args: fmt::Arguments) {
    use core::fmt::Write;
    interrupts::without_interrupts(|| {
        // Cố gắng lấy khóa
        let mut port = match SERIAL1.try_lock() {
            Some(lock) => lock,
            None => unsafe {
                // Nếu không lấy được (Deadlock?), phá khóa!
                SERIAL1.force_unlock();
                SERIAL1.lock()
            },
        };

        // In ra với tiền tố PANIC để dễ nhận biết
        let _ = port.write_fmt(format_args!("\n[FORCE_PRINT] "));
        let _ = port.write_fmt(args);
    });
}

// Macro serial_print
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::_print(format_args!($($arg)*));
    };
}

// Macro serial_println
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(concat!($fmt, "\n"), $($arg)*));
}
