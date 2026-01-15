use shared::serial_println;

// Define Testable Trait
pub trait Testable {
    fn run(&self);
}

impl<T> Testable for T
where
    T: Fn(),
{
    fn run(&self) {
        serial_println!("{}...\t", core::any::type_name::<T>());
        self();
        serial_println!("[ok]");
    }
}

// Enum for QEMU Exit Code
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub fn exit_qemu(exit_code: QemuExitCode) {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut port = Port::new(0xf4);
        port.write(exit_code as u32);
    }
}

// Global Panic Handler for Test Mode
pub fn panic(info: &core::panic::PanicInfo) -> ! {
    serial_println!("[failed]\n");
    serial_println!("Error: {}\n", info);
    exit_qemu(QemuExitCode::Failed);
    loop {}
}

// Manual Test Registry
pub fn run_tests() {
    serial_println!("Running Kernel Tests...");

    let tests: &[&dyn Testable] = &[
        &trivial_assertion,
        &simple_arithmetic,
        // Add more manual tests here
    ];

    for test in tests {
        test.run();
    }

    exit_qemu(QemuExitCode::Success);
}

// === TEST CASES ===

fn trivial_assertion() {
    assert_eq!(1, 1);
}

fn simple_arithmetic() {
    assert_eq!(2 + 2, 4);
}
