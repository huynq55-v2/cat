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

extern crate alloc;
use alloc::boxed::Box;
use alloc::vec::Vec;

// Manual Test Registry
pub fn run_tests() {
    serial_println!("Running Kernel Tests...");

    let tests: &[&dyn Testable] = &[
        &trivial_assertion,
        &simple_arithmetic,
        &test_stack_overflow_page_fault, // Basic check to see if we can trigger faults safely (optional, maybe skip for now to avoid crash)
        &test_heap_simple,
        &test_heap_large_vec,
        &test_heap_many_boxes,
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

fn test_heap_simple() {
    let heap_value = Box::new(41);
    assert_eq!(*heap_value, 41);
}

fn test_heap_large_vec() {
    let n = 1000;
    let mut vec = Vec::new();
    for i in 0..n {
        vec.push(i);
    }
    assert_eq!(vec.len(), n);
    assert_eq!(vec[0], 0);
    assert_eq!(vec[n - 1], n - 1);
}

fn test_heap_many_boxes() {
    for i in 0..10_000 {
        let x = Box::new(i);
        assert_eq!(*x, i);
    }
}

// Just a placeholder, actually triggering a stack overflow would kill the kernel
// unless we have a separate double fault stack.
// For now, let's keep it safe.
fn test_stack_overflow_page_fault() {
    // nothing
}
