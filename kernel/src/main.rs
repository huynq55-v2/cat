#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

use pc_keyboard::{DecodedKey, HandleControl, Keyboard, ScancodeSet1, layouts};
use shared::{panic::panic_handler_impl, serial_print, serial_println};

mod gdt;
mod heap_allocator;
mod interrupts;
mod layout;
mod pml4;
mod pmm;

extern crate alloc;

#[unsafe(no_mangle)]
pub extern "C" fn _start(
    mmap_addr_phys: u64,
    mmap_len: u64,
    desc_size: u64,
    hhdm_offset: u64,
    max_phys_addr: u64,
) -> ! {
    serial_println!("Hello from Kernel!");

    gdt::init();
    serial_println!("GDT & TSS initialized.");

    interrupts::init_idt();
    serial_println!("IDT initialized.");

    interrupts::PICS.initialize();

    serial_println!("PICS initialized.");

    pmm::init(
        mmap_addr_phys,
        mmap_len,
        desc_size,
        hhdm_offset,
        max_phys_addr,
    );
    let mut frame_allocator = pmm::KernelFrameAllocator;
    let mut mapper = unsafe { pml4::init_mapper(hhdm_offset) };

    interrupts::init_timer();
    serial_println!("PIT Timer initialized.");

    heap_allocator::init_heap(&mut mapper, &mut frame_allocator)
        .expect("Heap initialization failed");
    serial_println!("Heap is ready!");

    x86_64::instructions::interrupts::enable();
    serial_println!("Interrupts enabled!");

    serial_println!("System Ready. Try typing on QEMU window...");

    let mut keyboard = Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    );

    let mut last_tick = 0;

    loop {
        x86_64::instructions::interrupts::disable();

        let scancode = interrupts::pop_scancode();

        let current_ticks = unsafe { core::ptr::read_volatile(&raw const interrupts::TICKS) };

        x86_64::instructions::interrupts::enable();

        if current_ticks > last_tick && current_ticks % 20 == 0 {
            serial_print!(".");
            last_tick = current_ticks;
        }

        match scancode {
            Some(code) => {
                x86_64::instructions::interrupts::enable();

                if let Ok(Some(key_event)) = keyboard.add_byte(code)
                    && let Some(key) = keyboard.process_keyevent(key_event)
                {
                    match key {
                        DecodedKey::Unicode(character) => serial_print!("{}", character),
                        DecodedKey::RawKey(key) => serial_print!("{:?}", key),
                    }
                }
            }
            None => {
                x86_64::instructions::interrupts::enable_and_hlt();
            }
        }
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    panic_handler_impl(info);
}
