#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::writer::_print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($fmt:expr, $($arg:tt)*) => ($crate::print!(concat!($fmt, "\n"), $($arg)*));
    ($fmt:expr) => ($crate::print!(concat!($fmt, "\n")));
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    // Lock interrupts to avoid deadlock when both interrupt handler and kernel want to print
    interrupts::without_interrupts(|| {
        // 1. Print to Serial (always prioritize because it is the most stable for debugging)
        shared::serial::_print(args);

        // 2. Print to Screen (GOP)
        if let Some(writer) = &mut *crate::screen::WRITER.lock() {
            let _ = writer.write_fmt(args);
        }
    });
}
