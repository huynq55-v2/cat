// Import fmt for formatting strings
use core::fmt;
// Import lazy_static to initialize static variables lazily
use lazy_static::lazy_static;
// Import Mutex for thread-safe access to the serial port
use spin::Mutex;
// Import SerialPort driver
use uart_16550::SerialPort;
// Import interrupt instructions
use x86_64::instructions::interrupts;

// Define a lazy static wrapper for the serial port
// This ensures the serial port is initialized only when first accessed
lazy_static! {
    // The main serial port interface, protected by a Mutex (spinlock)
    // 0x3F8 is the standard I/O port address for COM1
    pub static ref SERIAL1: Mutex<SerialPort> = {
        // Create a new SerialPort instance for COM1
        // unsafe is needed because we are constructing a raw port address
        let mut serial_port = unsafe { SerialPort::new(0x3F8) };
        // Initialize the serial port
        serial_port.init();
        // Wrap it in a Mutex so it can be safely shared across the kernel
        Mutex::new(serial_port)
    };
}

// Hidden function used by the printing macros
// This builds the arguments and sends them to the serial port
#[doc(hidden)]
pub fn _print(args: ::core::fmt::Arguments) {
    // Import the Write trait to use write_fmt
    use core::fmt::Write;

    // Disable interrupts while printing
    // This is CRITICAL to prevent deadlocks:
    // If an interrupt occurs while we hold the lock, and the interrupt handler
    // tries to print, it would try to acquire the same lock, causing a deadlock.
    interrupts::without_interrupts(|| {
        // Acquire the lock on the serial port
        let mut serial = SERIAL1.lock();

        // Write the formatted string to the serial port
        serial.write_fmt(args).expect("Printing to serial failed");
    });
}

// Special print function for panics
// It tries to force-unlock the serial port if it's locked, to ensure the message gets out
pub fn print_panic(args: fmt::Arguments) {
    use core::fmt::Write;
    // Disable interrupts to ensure atomicity
    interrupts::without_interrupts(|| {
        // Try to lock normally first
        let mut port = match SERIAL1.try_lock() {
            Some(lock) => lock,
            None => unsafe {
                // If locked, force unlock it!
                // This is unsafe but necessary during a panic to ensure the message is seen.
                // The previous owner of the lock is dead/panicking anyway.
                SERIAL1.force_unlock();
                SERIAL1.lock()
            },
        };

        // Print a prefix indicating we forced it (optional, but good for debugging context)
        let _ = port.write_fmt(format_args!("\n[FORCE_PRINT] "));
        // Print the actual panic message
        let _ = port.write_fmt(args);
    });
}

// Macro for printing to the serial port (like print!)
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        // Call the helper function with formatted arguments
        $crate::serial::_print(format_args!($($arg)*));
    };
}

// Macro for printing with a newline (like println!)
#[macro_export]
macro_rules! serial_println {
    // Empty case prints just a newline
    () => ($crate::serial_print!("\n"));
    // Format string + newline
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    // Format string + arguments + newline
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(concat!($fmt, "\n"), $($arg)*));
}
