#![no_std] // No standard library
#![no_main] // No standard main function
#![feature(abi_x86_interrupt)] // Enable x86-interrupt ABI for IDT handlers

#[macro_use]
mod writer;

// Imports
use pc_keyboard::{DecodedKey, HandleControl, Keyboard, ScancodeSet1, layouts};
use shared::{BootInfo, panic::panic_handler_impl};

// Module Declarations
mod gdt;
mod heap_allocator;
mod interrupts;
mod pml4;
mod pmm;
mod screen;

// External Crate for Heap Allocation
extern crate alloc;

// The Kernel Entry Point
// This function is called by the UEFI Bootloader
#[unsafe(no_mangle)] // Ensure the symbol name is unique
pub extern "C" fn _start(boot_info: &'static BootInfo) -> ! {
    screen::init(boot_info.framebuffer);

    // Clear screen with the specified color
    screen::clear_screen(0x0000FF);

    // Set text scale and color
    screen::set_scale(3);
    screen::set_text_color(0xFF0000);
    println!("WELCOME TO MY OS");

    screen::reset_style();

    // Initialize Global Descriptor Table (GDT) and Task State Segment (TSS)
    gdt::init();
    println!("GDT & TSS initialized.");

    // Initialize Interrupt Descriptor Table (IDT)
    interrupts::init_idt();
    println!("IDT initialized.");

    // Initialize Programmable Interrupt Controllers (PICs)
    interrupts::PICS.initialize();
    println!("PICS initialized.");

    // Initialize Physical Memory Manager (PMM)
    pmm::init(
        boot_info.memory_map_addr,
        boot_info.memory_map_len,
        boot_info.memory_map_desc_size,
        boot_info.hhdm_offset,
        boot_info.max_phys_memory,
    );
    // Create an instance of our frame allocator
    let mut frame_allocator = pmm::KernelFrameAllocator;

    // Initialize the Virtual Memory Mapper using PMM and HHDM offset
    let mut mapper = unsafe { pml4::init_mapper(boot_info.hhdm_offset) };

    // Initialize Programmable Interval Timer (PIT)
    interrupts::init_timer();
    println!("PIT Timer initialized.");

    // Initialize the Heap Allocator
    // We pass the mapper and frame allocator so it can map new pages for the heap
    heap_allocator::init_heap(&mut mapper, &mut frame_allocator)
        .expect("Heap initialization failed");
    println!("Heap is ready!");
}

// Panic Handler
// Called on panic!(), prints error info and halts
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    panic_handler_impl(info);
}
