use core::panic::PanicInfo;

pub fn panic_handler_impl(_info: &PanicInfo) -> ! {
    #[cfg(feature = "serial")]
    {
        // Dùng hàm in riêng, không dùng serial_println! thường
        crate::serial::print_panic(format_args!("PANIC OCCURRED: {}\n", _info));
    }

    loop {
        #[cfg(feature = "serial")]
        unsafe {
            core::arch::asm!("hlt");
        }
    }
}
