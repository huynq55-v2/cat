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

// --- 2. ENTRY POINT ---
#[unsafe(no_mangle)]
pub extern "C" fn _start(
    mmap_addr_phys: u64,
    mmap_len: u64,
    desc_size: u64,
    hhdm_offset: u64,
    max_phys_addr: u64,
) -> ! {
    serial_println!("Hello from Kernel!");

    // 1. Init GDT & TSS
    gdt::init();
    serial_println!("GDT & TSS initialized.");

    // 2. Init Interrupts (IDT)
    interrupts::init_idt();
    serial_println!("IDT initialized.");

    // 3. Init PICS (Hardware Interrupts)
    interrupts::PICS.initialize();

    serial_println!("PICS initialized.");

    // [FIX 1] KHÔNG bật ngắt ở đây! Vì Heap chưa có.
    // x86_64::instructions::interrupts::enable(); <--- XÓA DÒNG NÀY

    // 4. Init Memory (PMM & Paging)
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

    // 5. Init Heap
    heap_allocator::init_heap(&mut mapper, &mut frame_allocator)
        .expect("Heap initialization failed");
    serial_println!("Heap is ready!");

    // [FIX 1] Bật ngắt ở đây mới an toàn (Heap đã sẵn sàng cho VecDeque)
    x86_64::instructions::interrupts::enable();
    serial_println!("Interrupts enabled!");

    serial_println!("System Ready. Try typing on QEMU window...");

    let mut keyboard = Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    );

    // Biến lưu thời điểm lần in cuối cùng để tránh in trùng lặp
    let mut last_tick = 0;

    // VÒNG LẶP CHÍNH (Consumer)
    loop {
        // Tắt ngắt trước khi kiểm tra Buffer
        x86_64::instructions::interrupts::disable();

        // Lấy scancode từ Buffer tĩnh (Hàm mới trong interrupts.rs)
        let scancode = interrupts::pop_scancode();

        // [QUAN TRỌNG] Dùng read_volatile để bắt buộc CPU đọc lại giá trị từ RAM
        // Nếu không, Compiler có thể tự ý "tối ưu" và nghĩ rằng TICKS không bao giờ đổi.
        let current_ticks = unsafe { core::ptr::read_volatile(&raw const interrupts::TICKS) };

        // Bật lại ngắt
        x86_64::instructions::interrupts::enable();

        // 3. XỬ LÝ TIMER (LOGIC MỚI)
        // Chỉ in khi thời gian ĐÃ THAY ĐỔI và chạm mốc mỗi giây (20 ticks)
        if current_ticks > last_tick && current_ticks % 20 == 0 {
            serial_print!(".");
            last_tick = current_ticks; // Cập nhật mốc để không in lại lần nữa trong tick này
        }

        match scancode {
            Some(code) => {
                // Có phím -> Bật lại ngắt ngay để xử lý
                x86_64::instructions::interrupts::enable();

                // Xử lý PC-Keyboard
                if let Ok(Some(key_event)) = keyboard.add_byte(code) {
                    if let Some(key) = keyboard.process_keyevent(key_event) {
                        match key {
                            DecodedKey::Unicode(character) => serial_print!("{}", character),
                            DecodedKey::RawKey(key) => serial_print!("{:?}", key),
                        }
                    }
                }
            }
            None => {
                // Không có phím -> Ngủ (Bật ngắt + Hlt nguyên tử)
                x86_64::instructions::interrupts::enable_and_hlt();
            }
        }
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    panic_handler_impl(info);
}
