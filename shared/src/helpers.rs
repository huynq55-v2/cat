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

#[inline(always)]
pub fn is_canonical(addr: u64) -> bool {
    let sign = (addr >> 47) & 1;
    if sign == 0 {
        addr >> 48 == 0
    } else {
        addr >> 48 == 0xffff
    }
}

#[inline]
pub fn align_up(x: u64, align: u64) -> u64 {
    (x + align - 1) & !(align - 1)
}
