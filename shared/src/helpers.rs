// 1. Định nghĩa hlt_loop ngay tại đây để tránh lỗi crate::hlt_loop not found
pub fn hlt_loop() -> ! {
    loop {
        unsafe { core::arch::asm!("hlt") }
    }
}
