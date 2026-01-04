#![no_std] // No standard library
#![no_main] // No standard main function
#![feature(abi_x86_interrupt)] // Enable x86-interrupt ABI for IDT handlers

// Imports
use pc_keyboard::{DecodedKey, HandleControl, Keyboard, ScancodeSet1, layouts};
use shared::{panic::panic_handler_impl, serial_print, serial_println};

// Module Declarations
mod gdt;
mod heap_allocator;
mod interrupts;
mod layout;
mod pml4;
mod pmm;

// External Crate for Heap Allocation
extern crate alloc;

// The Kernel Entry Point
// This function is called by the UEFI Bootloader
#[unsafe(no_mangle)] // Ensure the symbol name is unique
pub extern "C" fn _start(
    mmap_addr_phys: u64, // Physical address of UEFI memory map
    mmap_len: u64,       // Length of memory map
    desc_size: u64,      // Size of each descriptor
    hhdm_offset: u64,    // Higher Half Direct Map Offset
    max_phys_addr: u64,  // Maximum physical address detected
) -> ! {
    serial_println!("Hello from Kernel!");

    // Initialize Global Descriptor Table (GDT) and Task State Segment (TSS)
    gdt::init();
    serial_println!("GDT & TSS initialized.");

    // Initialize Interrupt Descriptor Table (IDT)
    interrupts::init_idt();
    serial_println!("IDT initialized.");

    // Initialize Programmable Interrupt Controllers (PICs)
    interrupts::PICS.initialize();
    serial_println!("PICS initialized.");

    // Initialize Physical Memory Manager (PMM)
    pmm::init(
        mmap_addr_phys,
        mmap_len,
        desc_size,
        hhdm_offset,
        max_phys_addr,
    );
    // Create an instance of our frame allocator
    let mut frame_allocator = pmm::KernelFrameAllocator;

    // Initialize the Virtual Memory Mapper using PMM and HHDM offset
    let mut mapper = unsafe { pml4::init_mapper(hhdm_offset) };

    // Initialize Programmable Interval Timer (PIT)
    interrupts::init_timer();
    serial_println!("PIT Timer initialized.");

    // Initialize the Heap Allocator
    // We pass the mapper and frame allocator so it can map new pages for the heap
    heap_allocator::init_heap(&mut mapper, &mut frame_allocator)
        .expect("Heap initialization failed");
    serial_println!("Heap is ready!");

    // Enable CPU Interrupts
    x86_64::instructions::interrupts::enable();
    serial_println!("Interrupts enabled!");

    serial_println!("System Ready. Try typing on QEMU window...");

    // Initialize Keyboard Decoder (US Layout, Scancode Set 1)
    let mut keyboard = Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    );

    let mut last_tick = 0;

    // Main Kernel Loop
    loop {
        // Disable interrupts while reading shared data (to prevent race conditions)
        x86_64::instructions::interrupts::disable();

        // Check for new scancode from interrupt handler
        let scancode = interrupts::pop_scancode();

        // Read current timer ticks safely
        let current_ticks = unsafe { core::ptr::read_volatile(&raw const interrupts::TICKS) };

        // Re-enable interrupts
        x86_64::instructions::interrupts::enable();

        // Print a dot every 20 ticks (approx 1 second)
        if current_ticks > last_tick && current_ticks % 20 == 0 {
            serial_print!(".");
            last_tick = current_ticks;
        }

        // Process scancode if available
        match scancode {
            Some(code) => {
                // Ensure interrupts are enabled so we don't block
                x86_64::instructions::interrupts::enable();

                // Decode the scancode into a key event
                if let Ok(Some(key_event)) = keyboard.add_byte(code)
                    && let Some(key) = keyboard.process_keyevent(key_event)
                {
                    // Print the decoded character
                    match key {
                        DecodedKey::Unicode(character) => serial_print!("{}", character),
                        DecodedKey::RawKey(key) => serial_print!("{:?}", key),
                    }
                }
            }
            None => {
                // If no input, halt CPU until next interrupt (save power)
                x86_64::instructions::interrupts::enable_and_hlt();
            }
        }
    }
}

// Panic Handler
// Called on panic!(), prints error info and halts
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    panic_handler_impl(info);
}
