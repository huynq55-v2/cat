use core::panic::PanicInfo;

// The implementation of the panic handler
// This function is called when a panic occurs
pub fn panic_handler_impl(info: &PanicInfo) -> ! {
    #[cfg(feature = "serial")]
    {
        // in location nếu có
        if let Some(location) = info.location() {
            crate::serial::print_panic(format_args!(
                "panic at {}:{}:{}\n",
                location.file(),
                location.line(),
                location.column(),
            ));
        } else {
            crate::serial::print_panic(format_args!("panic at <unknown location>\n"));
        }

        // in message
        crate::serial::print_panic(format_args!("message: {}\n", info.message()));
    }

    loop {
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}
