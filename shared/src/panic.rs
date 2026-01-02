use core::panic::PanicInfo;

pub fn panic_handler_impl(_info: &PanicInfo) -> ! {
    // Write to serial port
    #[cfg(feature = "serial")]
    {
        use crate::serial_println;
        serial_println!("[PANIC]: {}", _info);
    }

    loop {
        #[cfg(feature = "serial")]
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}