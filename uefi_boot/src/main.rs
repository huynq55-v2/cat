#![no_std]
#![no_main]

use core::slice;
use log::info;
use shared::panic::panic_handler_impl;
use uefi::boot::{AllocateType, MemoryType};
use uefi::mem::memory_map::MemoryMap;
use uefi::prelude::*;
use uefi::proto::media::file::{File, FileAttribute, FileInfo, FileMode};
use x86_64::structures::paging::{
    FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size2MiB,
    Size4KiB,
};
use x86_64::{PhysAddr, VirtAddr};
use xmas_elf::ElfFile;
use xmas_elf::program::Type;

/// Một bộ cấp phát bộ nhớ đơn giản: Lấy một cục to, cắt nhỏ dần.
struct BumpAllocator {
    next: u64,
    end: u64,
}

impl BumpAllocator {
    /// Khởi tạo allocator với vùng nhớ đã được cấp phát sẵn
    /// start: Địa chỉ vật lý bắt đầu
    /// pages: Số lượng trang đã cấp phát
    unsafe fn new(start: u64, pages: usize) -> Self {
        let size = (pages as u64) * 4096;
        Self {
            next: start,
            end: start + size,
        }
    }
}

unsafe impl FrameAllocator<Size4KiB> for BumpAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        // Kiểm tra xem còn bộ nhớ không
        if self.next >= self.end {
            // Hết bộ nhớ dự trữ!
            // Trong bootloader đơn giản, ta có thể panic hoặc trả về None.
            return None;
        }

        // Lấy địa chỉ hiện tại làm frame trả về
        let frame_addr = PhysAddr::new(self.next);
        let frame = PhysFrame::containing_address(frame_addr);

        // Tăng con trỏ lên trang tiếp theo (4KB)
        self.next += 4096;

        Some(frame)
    }
}

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();
    info!("Hello from UEFI Bootloader!");

    // 1. Access the SimpleFileSystem
    let image_handle = boot::image_handle();
    let mut fs =
        boot::get_image_file_system(image_handle).expect("Failed to get image file system");
    let mut root = fs.open_volume().expect("Failed to open root volume");

    // 2. Open the kernel file
    let mut kernel_file = root
        .open(
            uefi::cstr16!("kernel"),
            FileMode::Read,
            FileAttribute::empty(),
        )
        .expect("Failed to open 'kernel' file")
        .into_regular_file()
        .expect("Kernel file is not a regular file");

    info!("Found kernel file");

    // 3. Read kernel file into memory
    let mut info_buf = [0u8; 128];
    let file_info = kernel_file
        .get_info::<FileInfo>(&mut info_buf)
        .expect("Failed to get file info");
    let file_size = file_info.file_size();

    info!("Kernel file size: {} bytes", file_size);

    let pages_needed = (file_size as usize + 0xfff) / 0x1000;

    // Allocate memory for the raw file
    let file_buffer_addr = boot::allocate_pages(
        AllocateType::AnyPages,
        MemoryType::LOADER_DATA,
        pages_needed,
    )
    .expect("Failed to allocate pages for kernel file");

    let file_buffer =
        unsafe { slice::from_raw_parts_mut(file_buffer_addr.as_ptr(), pages_needed * 0x1000) };

    let len = kernel_file
        .read(file_buffer)
        .expect("Failed to read kernel file");

    let kernel_data = &file_buffer[..len];

    // 4. Parse ELF
    let elf = ElfFile::new(kernel_data).expect("Failed to parse ELF");
    let entry_point = elf.header.pt2.entry_point();
    info!("ELF Entry point: {:#x}", entry_point);

    // -----------------------------------------------------------------------
    // PAGING SETUP (OPTIMIZED)
    // -----------------------------------------------------------------------

    // 1. Cấp phát một vùng nhớ LỚN từ UEFI để dùng làm Page Table
    // Tính toán sơ bộ:
    // - Kernel của bạn nhỏ, mapping cơ bản cần khoảng 10-20 trang (PML4, PDPT, PD, PT).
    // - Stack cần 20 trang.
    // - Bootloader map cần 512 trang (2MB).
    // -> Cấp hẳn 1024 trang (4MB) cho thoải mái, tránh phân mảnh.
    const PAGE_TABLE_POOL_SIZE: usize = 1024;

    let pool_addr = boot::allocate_pages(
        AllocateType::AnyPages,
        MemoryType::LOADER_DATA,
        PAGE_TABLE_POOL_SIZE,
    )
    .expect("Failed to allocate memory pool for Page Tables");

    // 2. Khởi tạo Bump Allocator với vùng nhớ vừa xin
    // Lưu ý: FrameAllocator bây giờ hoàn toàn chạy trên RAM cục bộ,
    // không gọi Boot Services nữa -> Cực nhanh!
    let mut frame_allocator =
        unsafe { BumpAllocator::new(pool_addr.as_ptr() as u64, PAGE_TABLE_POOL_SIZE) };

    info!(
        "Initialized BumpAllocator at {:#x} with {} pages",
        pool_addr.as_ptr() as u64,
        PAGE_TABLE_POOL_SIZE
    );

    // 3. Allocate PML4 (Lấy trang đầu tiên từ pool)
    let pml4_frame = frame_allocator
        .allocate_frame()
        .expect("Failed to allocate PML4 (Pool empty?)");

    let pml4_phys = pml4_frame.start_address();
    let pml4 = unsafe { &mut *(pml4_phys.as_u64() as *mut PageTable) };
    pml4.zero();

    // Context for mapping
    // We utilize the observation that UEFI identity maps physical memory.
    // So `OffsetPageTable` with offset 0 works access physical memory.
    let mut mapper = unsafe { OffsetPageTable::new(pml4, VirtAddr::new(0)) };

    // 5. Map Kernel Segments
    for ph in elf.program_iter() {
        if let Ok(Type::Load) = ph.get_type() {
            let mem_size = ph.mem_size();
            let file_size = ph.file_size();
            let virt_addr = ph.virtual_addr();
            let offset = ph.offset();

            info!(
                "Loading segment: virt={:#x}, mem_size={:#x}, file_size={:#x}",
                virt_addr, mem_size, file_size
            );

            // Allocate physical memory for this segment
            let pages = (mem_size as usize + 0xfff) / 0x1000;
            let phys_addr =
                boot::allocate_pages(AllocateType::AnyPages, MemoryType::LOADER_DATA, pages)
                    .expect("Failed to allocate pages for segment");

            let segment_slice =
                unsafe { slice::from_raw_parts_mut(phys_addr.as_ptr(), pages * 0x1000) };

            // Copy data from file buffer
            let start = offset as usize;
            let end = start + file_size as usize;
            segment_slice[..file_size as usize].copy_from_slice(&kernel_data[start..end]);

            // Zero out remaining memory (BSS)
            if mem_size > file_size {
                for i in file_size as usize..mem_size as usize {
                    segment_slice[i] = 0;
                }
            }

            // Map content to virtual address
            let start_page = Page::<Size4KiB>::containing_address(VirtAddr::new(virt_addr));
            let end_page =
                Page::<Size4KiB>::containing_address(VirtAddr::new(virt_addr + mem_size - 1));

            let mut flags = PageTableFlags::PRESENT;
            if ph.flags().is_write() {
                flags |= PageTableFlags::WRITABLE;
            }

            let mut frame_addr = phys_addr.as_ptr() as u64;

            for page in Page::range_inclusive(start_page, end_page) {
                let frame = PhysFrame::containing_address(PhysAddr::new(frame_addr));
                unsafe {
                    mapper
                        .map_to(page, frame, flags, &mut frame_allocator)
                        .expect("Failed to map kernel page")
                        .flush();
                }
                frame_addr += 4096;
            }

            info!("Segment mapped.");
        }
    }

    // 6. Map Stack
    // Create a stack at top of higher half
    const KERNEL_BASE: u64 = 0xffffffff80000000;
    const STACK_TOP: u64 = KERNEL_BASE - 0x1000;

    let stack_start = VirtAddr::new(STACK_TOP);
    let stack_size = 20 * 1024; // 20KB
    let stack_bottom = stack_start - stack_size;
    let stack_bottom_page = Page::<Size4KiB>::containing_address(stack_bottom);
    let stack_top_page = Page::<Size4KiB>::containing_address(stack_start - 1u64);

    info!("Mapping stack at {:#x}", stack_start.as_u64());

    for page in Page::range_inclusive(stack_bottom_page, stack_top_page) {
        let frame = frame_allocator
            .allocate_frame()
            .expect("Failed to allocate stack frame");
        unsafe {
            mapper
                .map_to(
                    page,
                    frame,
                    PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                    &mut frame_allocator,
                )
                .expect("Failed to map stack")
                .flush();
        }
    }

    // --- FINAL PREPARATION ---

    // 1. TÍNH TOÁN DESCRIPTOR SIZE (FIXED)
    let desc_size = {
        let mmap = boot::memory_map(MemoryType::LOADER_DATA).expect("Failed to get temp mmap");
        let mut entries = mmap.entries();
        let e1 = entries.next().unwrap();
        let e2 = entries.next().unwrap();
        (e2 as *const _ as u64) - (e1 as *const _ as u64)
    };

    // 1. Lấy Memory Map tạm thời để quét Max RAM
    // Lưu ý: MemoryType::LOADER_DATA chỉ là loại ram cấp cho mmap này, không quan trọng lắm
    let mmap_storage = boot::memory_map(MemoryType::LOADER_DATA).expect("Failed to get memory map");

    let mut max_phys_addr = 0;

    // 2. Duyệt để tìm địa chỉ vật lý cuối cùng
    for descriptor in mmap_storage.entries() {
        let start = descriptor.phys_start;
        let size = descriptor.page_count * 4096;
        let end = start + size;

        if end > max_phys_addr {
            max_phys_addr = end;
        }
    }

    // Làm tròn max_phys_addr lên 2MB gần nhất để map cho đẹp (Alignment)
    // Ví dụ RAM lẻ 8GB + chút ít thì làm tròn lên để map trọn vẹn
    max_phys_addr = (max_phys_addr + 0x1fffff) & !0x1fffff;

    info!(
        "Detected Max Physical Address: {:#x} ({} MB)",
        max_phys_addr,
        max_phys_addr / 1024 / 1024
    );

    // -----------------------------------------------------------------------
    // HHDM MAPPING (Dynamic Size)
    // -----------------------------------------------------------------------
    const HHDM_OFFSET: u64 = 0xffff_8000_0000_0000;

    let start_frame = PhysFrame::<Size2MiB>::containing_address(PhysAddr::new(0));
    // Thay vì hardcode 4GB, ta dùng max_phys_addr vừa tìm được
    let end_frame = PhysFrame::<Size2MiB>::containing_address(PhysAddr::new(max_phys_addr - 1));

    info!("Mapping HHDM from 0 to {:#x}...", max_phys_addr);

    for frame in PhysFrame::range_inclusive(start_frame, end_frame) {
        let phys_addr = frame.start_address().as_u64();
        let virt_addr = phys_addr + HHDM_OFFSET;

        let page = Page::<Size2MiB>::containing_address(VirtAddr::new(virt_addr));
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

        unsafe {
            let _ = mapper
                .map_to(page, frame, flags, &mut frame_allocator)
                .ok()
                .map(|f| f.flush());
        }
    }

    info!("HHDM mapped successfully!");

    // -----------------------------------------------------------------------
    // [FIX] IDENTITY MAP ĐOẠN CODE HIỆN TẠI (TRAMPOLINE)
    // -----------------------------------------------------------------------
    // Bắt buộc phải có để CPU không bị crash ngay sau khi đổi CR3.

    let current_rip: u64;
    unsafe { core::arch::asm!("lea {}, [rip]", out(reg) current_rip) };

    // Làm tròn xuống đầu trang 4K
    let start_rip = current_rip & !0xfff;

    info!(
        "Identity mapping current execution code at {:#x}",
        start_rip
    );

    // Map khoảng 512 trang (2MB) xung quanh vị trí hiện tại cho chắc ăn
    // Để đảm bảo cả code asm! và stack hiện tại của bootloader đều nằm trong đó.
    for offset in 0..512 {
        let addr = start_rip + (offset * 0x1000);
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(addr));
        let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(addr));

        unsafe {
            let _ = mapper
                .map_to(
                    page,
                    frame,
                    PageTableFlags::PRESENT | PageTableFlags::WRITABLE, // Writable để stack hoạt động
                    &mut frame_allocator,
                )
                .ok()
                .map(|f| f.flush());
        }
    }

    // 3. Exit Boot Services
    let mmap = unsafe { boot::exit_boot_services(Some(MemoryType::LOADER_DATA)) };

    // 4. Chuẩn bị tham số
    // Lưu ý: entries().next() lấy phần tử đầu tiên để làm mốc địa chỉ
    let mmap_addr_phys = mmap
        .entries()
        .next()
        .map(|d| d as *const _ as u64)
        .unwrap_or(0);
    let mmap_len = mmap.entries().len() as u64;

    let pml4_phys = pml4_frame.start_address().as_u64();
    let stack_top = stack_start.as_u64();

    // 5. Atomic Context Switch
    unsafe {
        x86_64::instructions::interrupts::disable();

        core::arch::asm!(
            "mov cr3, {pml4}",      // 1. Đổi Page Table
            "mov rsp, {stack}",     // 2. Đổi Stack
            "xor rbp, rbp",         // 3. Xóa RBP
            "jmp {entry}",          // 4. Bay!

            pml4 = in(reg) pml4_phys,
            stack = in(reg) stack_top,
            entry = in(reg) entry_point,

            // Tham số Kernel: fn(mmap_addr, mmap_len, desc_size)
            in("rdi") mmap_addr_phys,
            in("rsi") mmap_len,
            in("rdx") desc_size,
            in("rcx") HHDM_OFFSET,

            options(noreturn)
        );
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    panic_handler_impl(info)
}
