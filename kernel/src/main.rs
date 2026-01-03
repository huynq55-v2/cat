#![no_std]
#![no_main]

// Giả sử bạn đã có macro serial_println! từ shared library hoặc module
use shared::{panic::panic_handler_impl, serial_println};
mod heap_allocator;
mod layout;
mod pml4;
mod pmm;
mod spinlock;

extern crate alloc;

// --- 1. ĐỊNH NGHĨA LẠI CẤU TRÚC UEFI (Chuẩn C) ---
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct MemoryDescriptor {
    pub type_: u32,      // Loại bộ nhớ (BootServicesCode, Conventional, etc.)
    pub pad: u32,        // Padding (quan trọng để alignment đúng)
    pub phys_start: u64, // Địa chỉ vật lý
    pub virt_start: u64, // Địa chỉ ảo (thường bằng phys_start trong UEFI)
    pub page_count: u64, // Số lượng trang (4KB mỗi trang)
    pub attribute: u64,  // Thuộc tính (Read/Write/Exec...)
}

// Helper để in tên loại bộ nhớ cho dễ đọc
impl MemoryDescriptor {
    pub fn type_name(&self) -> &'static str {
        match self.type_ {
            0 => "Reserved",
            1 => "LoaderCode",
            2 => "LoaderData",
            3 => "BootServicesCode",
            4 => "BootServicesData",
            5 => "RuntimeServicesCode",
            6 => "RuntimeServicesData",
            7 => "ConventionalMemory", // <-- Đây là RAM trống bạn dùng được!
            8 => "UnusableMemory",
            9 => "ACPIReclaimMemory",
            10 => "ACPIMemoryNVS",
            11 => "MemoryMappedIO",
            12 => "MemoryMappedIOPortSpace",
            13 => "PalCode",
            14 => "PersistentMemory",
            _ => "Unknown",
        }
    }
}

// --- 2. ENTRY POINT ---
#[unsafe(no_mangle)]
pub extern "C" fn _start(
    mmap_addr_phys: u64,
    mmap_len: u64,
    desc_size: u64,
    hhdm_offset: u64,
    max_phys_addr: u64,
) -> ! {
    serial_println!("Hello from Kernel with Spinlock PMM!");
    serial_println!("HHDM Offset: {:#x}", hhdm_offset);
    serial_println!("--------------------------------------------------");
    let mmap_addr_virt = mmap_addr_phys + hhdm_offset;
    serial_println!(
        "MMap Virtual Address: {:#x}, Length: {}, Descriptor Size: {}",
        mmap_addr_virt,
        mmap_len,
        desc_size
    );
    let mut usable_pages = 0;
    let mut total_pages = 0;

    // --- 3. DUYỆT MEMORY MAP ---
    for i in 0..mmap_len {
        // TÍNH TOÁN ĐỊA CHỈ:
        // Địa chỉ của phần tử thứ i = Base + (i * desc_size)
        // Lưu ý: Phải dùng arithmetic trên u64 hoặc ép kiểu sang *const u8 để cộng byte.
        let addr = mmap_addr_virt + (i * desc_size);

        // Ép kiểu địa chỉ đó thành con trỏ MemoryDescriptor
        let desc = unsafe { &*(addr as *const MemoryDescriptor) };

        serial_println!(
            "Region {:3}: [{:#016x} - {:#016x}] {:20} ({} pages)",
            i,
            desc.phys_start,
            desc.phys_start + (desc.page_count * 4096),
            desc.type_name(),
            desc.page_count
        );

        // Thống kê bộ nhớ
        total_pages += desc.page_count;
        if desc.type_ == 7 {
            // ConventionalMemory
            usable_pages += desc.page_count;
        }
    }

    serial_println!("--------------------------------------------------");
    serial_println!("Total Memory: {} MB", (total_pages * 4096) / 1024 / 1024);
    serial_println!(
        "Free RAM (Usable): {} MB",
        (usable_pages * 4096) / 1024 / 1024
    );

    // 1. Init (Cần unsafe vì thao tác raw pointer bên trong init)
    pmm::init(
        mmap_addr_phys,
        mmap_len,
        desc_size,
        hhdm_offset,
        max_phys_addr,
    );

    let mut frame_allocator = pmm::KernelFrameAllocator;
    let mut mapper = unsafe { pml4::init_mapper(hhdm_offset) };

    heap_allocator::init_heap(&mut mapper, &mut frame_allocator)
        .expect("Heap initialization failed");

    serial_println!("Heap is ready!");

    use alloc::vec::Vec;
    serial_println!("Testing Vec...");
    let mut v = Vec::new();
    v.push(1);
    v.push(2);
    v.push(3);
    serial_println!("Vec content: {:?}", v);

    loop {
        unsafe { core::arch::asm!("hlt") }
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    panic_handler_impl(info);
}
