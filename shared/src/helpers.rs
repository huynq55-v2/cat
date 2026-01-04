pub fn hlt_loop() -> ! {
    loop {
        unsafe { core::arch::asm!("hlt") }
    }
}
