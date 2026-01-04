// Function to loop endlessly and halt the CPU
// This is used to save power when the OS has nothing to do or has panicked
pub fn hlt_loop() -> ! {
    // Infinite loop
    loop {
        // Execute the 'hlt' instruction
        // This stops the CPU until the next interrupt arrives
        unsafe { core::arch::asm!("hlt") }
    }
}
