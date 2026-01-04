// Import PanicInfo to get details about the panic (file, line, message)
use core::panic::PanicInfo;

// The implementation of the panic handler
// This function is called when a panic occurs
pub fn panic_handler_impl(_info: &PanicInfo) -> ! {
    // Check if the 'serial' feature is enabled
    #[cfg(feature = "serial")]
    {
        // Print the panic details to the serial port
        // This is useful for debugging since we might not have a screen driver yet
        crate::serial::print_panic(format_args!("PANIC OCCURRED: {}\n", _info));
    }

    // Enter an infinite loop to stop the system
    loop {
        // If 'serial' feature is enabled, halt the CPU
        #[cfg(feature = "serial")]
        unsafe {
            // Execute the 'hlt' instruction to save energy
            core::arch::asm!("hlt");
        }
    }
}
