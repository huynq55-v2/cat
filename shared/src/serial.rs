use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::SerialPort;
use x86_64::instructions::interrupts;

lazy_static! {
    pub static ref SERIAL1: Mutex<SerialPort> = {
        let mut serial_port = unsafe { SerialPort::new(0x3F8) };
        serial_port.init();
        Mutex::new(serial_port)
    };
}

#[doc(hidden)]
pub fn _print(args: ::core::fmt::Arguments) {
    use core::fmt::Write;

    interrupts::without_interrupts(|| {
        let mut serial = SERIAL1.lock();

        serial.write_fmt(args).expect("Printing to serial failed");

    });
}

pub fn print_panic(args: fmt::Arguments) {
    use core::fmt::Write;
    interrupts::without_interrupts(|| {
        let mut port = match SERIAL1.try_lock() {
            Some(lock) => lock,
            None => unsafe {
                SERIAL1.force_unlock();
                SERIAL1.lock()
            },
        };

        let _ = port.write_fmt(format_args!("\n[FORCE_PRINT] "));
        let _ = port.write_fmt(args);
    });
}

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::_print(format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(concat!($fmt, "\n"), $($arg)*));
}
