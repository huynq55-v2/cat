use core::fmt::{self, Write};
use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::SerialPort;

lazy_static! {
    /// SERIAL1 là port COM1, thread-safe
    pub static ref SERIAL1: Mutex<SerialPort> = {
        let mut serial = unsafe { SerialPort::new(0x3F8) }; // COM1
        serial.init();
        Mutex::new(serial)
    };
}

/// Hàm nội bộ in ra serial
#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    SERIAL1.lock()
        .write_fmt(args)
        .expect("Printing to serial failed");
}

/// Macro serial_print!
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::_print(format_args!($($arg)*));
    };
}

/// Macro serial_println!
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(
        concat!($fmt, "\n"), $($arg)*
    ));
}
