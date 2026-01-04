use core::panic::PanicInfo;

pub fn panic_handler_impl(_info: &PanicInfo) -> ! {
    #[cfg(feature = "serial")]
    {
        crate::serial::print_panic(format_args!("PANIC OCCURRED: {}\n", _info));
    }

    loop {
        #[cfg(feature = "serial")]
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}
