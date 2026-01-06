// ELF Loader Module
// This module loads an ELF64 executable into user memory and prepares for user mode execution

use x86_64::VirtAddr;
use x86_64::structures::paging::{
    FrameAllocator, Mapper, OffsetPageTable, Page, PageTableFlags, Size4KiB,
};
use xmas_elf::{
    ElfFile, header,
    program::{ProgramHeader, Type},
};

// User space base address for Position-Independent Executables
// We load PIE executables at a high virtual address in the user space
const USER_BASE_ADDR: u64 = 0x40_0000; // 4 MB - standard user space base for PIE

// User stack configuration
const USER_STACK_BOTTOM: u64 = 0x7FFF_FFFF_0000; // Top of user space
const USER_STACK_SIZE: u64 = 16 * 4096; // 64 KB stack

// Include the user ELF binary at compile time
// Change this path to load a different program
static USER_ELF_BYTES: &[u8] = include_bytes!("../../user_space/hello");

// Global variable to store HHDM offset
static mut HHDM_OFFSET: u64 = 0;

/// Initialize the ELF loader with HHDM offset
pub fn init_hhdm(offset: u64) {
    unsafe {
        HHDM_OFFSET = offset;
    }
}

/// Get the HHDM offset
fn get_hhdm_offset() -> u64 {
    unsafe { HHDM_OFFSET }
}

/// Load the user ELF executable into memory
/// Returns the entry point virtual address
pub fn load_user_elf(
    mapper: &mut OffsetPageTable<'static>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> VirtAddr {
    // Parse the ELF file
    let elf = ElfFile::new(USER_ELF_BYTES).expect("Failed to parse ELF file");

    // Verify this is a valid ELF64 executable
    assert!(
        elf.header.pt1.magic == [0x7F, b'E', b'L', b'F'],
        "Invalid ELF magic"
    );

    // Determine if this is a PIE or a regular executable
    let is_pie = elf.header.pt2.type_().as_type() == header::Type::SharedObject;

    // For PIE, we add a base address. For EXEC, we use the addresses as-is.
    let base_addr = if is_pie {
        println!(
            "[ELF] PIE executable detected, loading at base {:#x}",
            USER_BASE_ADDR
        );
        USER_BASE_ADDR
    } else {
        println!("[ELF] Static executable detected");
        0
    };

    // Load each LOAD segment into memory
    for program_header in elf.program_iter() {
        if let Ok(Type::Load) = program_header.get_type() {
            load_segment(mapper, frame_allocator, &elf, &program_header, base_addr);
        }
    }

    // Pre-map a region for mmap pool (used by musl for signal stacks, etc.)
    // This is a simple approach - a real OS would map on demand
    const MMAP_POOL_START: u64 = 0x480000;
    const MMAP_POOL_END: u64 = 0x500000; // 512KB pool

    let pool_start_page = Page::<Size4KiB>::containing_address(VirtAddr::new(MMAP_POOL_START));
    let pool_end_page = Page::<Size4KiB>::containing_address(VirtAddr::new(MMAP_POOL_END - 1));

    let pool_flags = PageTableFlags::PRESENT
        | PageTableFlags::USER_ACCESSIBLE
        | PageTableFlags::WRITABLE
        | PageTableFlags::NO_EXECUTE;

    let hhdm = get_hhdm_offset();

    for page in Page::range_inclusive(pool_start_page, pool_end_page) {
        if mapper.translate_page(page).is_ok() {
            continue; // Already mapped
        }

        let frame = frame_allocator
            .allocate_frame()
            .expect("Failed to allocate frame for mmap pool");

        unsafe {
            mapper
                .map_to(page, frame, pool_flags, frame_allocator)
                .expect("Failed to map mmap pool page")
                .flush();

            // Zero the page
            let frame_ptr = (frame.start_address().as_u64() + hhdm) as *mut u8;
            core::ptr::write_bytes(frame_ptr, 0, 4096);
        }
    }

    println!(
        "[ELF] mmap pool pre-mapped: {:#x} - {:#x}",
        MMAP_POOL_START, MMAP_POOL_END
    );

    // ALSO Pre-map a region for Heap (brk)
    // Current sys_brk starts at 0x8000000
    const HEAP_START: u64 = 0x8000000;
    const HEAP_END: u64 = 0x8100000; // 1MB heap

    let heap_start_page = Page::<Size4KiB>::containing_address(VirtAddr::new(HEAP_START));
    let heap_end_page = Page::<Size4KiB>::containing_address(VirtAddr::new(HEAP_END - 1));

    for page in Page::range_inclusive(heap_start_page, heap_end_page) {
        if mapper.translate_page(page).is_ok() {
            continue;
        }

        let frame = frame_allocator
            .allocate_frame()
            .expect("Failed to allocate frame for heap");

        unsafe {
            mapper
                .map_to(page, frame, pool_flags, frame_allocator) // Re-use pool flags (RW, USER)
                .expect("Failed to map heap page")
                .flush();
            let frame_ptr = (frame.start_address().as_u64() + hhdm) as *mut u8;
            core::ptr::write_bytes(frame_ptr, 0, 4096);
        }
    }
    println!("[ELF] Heap pre-mapped: {:#x} - {:#x}", HEAP_START, HEAP_END);

    // Process relocations for PIE executable
    // We intentionally skip kernel-side relocations effectively letting musl handle it
    // itself using the information provided in the Auxiliary Vector (AT_PHDR).
    // If we were to apply relocations here, we would get double-relocation because
    // musl also applies them by adding the base address.
    if is_pie {
        // process_relocations(mapper, &elf, base_addr);
        println!("[ELF] Skipping kernel relocations - expecting user runtime self-relocation");
    }

    // Calculate the actual entry point
    let entry_offset = elf.header.pt2.entry_point();
    let entry_point = VirtAddr::new(base_addr + entry_offset);

    println!("[ELF] Entry point at {:#x}", entry_point.as_u64());

    entry_point
}

/// Load a single program segment into memory
fn load_segment(
    mapper: &mut OffsetPageTable<'static>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    elf: &ElfFile,
    ph: &ProgramHeader,
    base_addr: u64,
) {
    // Get segment information
    let segment_vaddr = base_addr + ph.virtual_addr(); // Relocated virtual address
    let segment_memsz = ph.mem_size();
    let segment_filesz = ph.file_size();
    let segment_offset = ph.offset() as usize;
    let flags = ph.flags();

    // Skip empty segments
    if segment_memsz == 0 {
        return;
    }

    println!(
        "[ELF] Loading segment: vaddr={:#x}, memsz={:#x}, filesz={:#x}",
        segment_vaddr, segment_memsz, segment_filesz
    );

    // Calculate page-aligned boundaries
    let start_page = Page::<Size4KiB>::containing_address(VirtAddr::new(segment_vaddr));
    let end_addr = segment_vaddr + segment_memsz;
    let end_page = Page::<Size4KiB>::containing_address(VirtAddr::new(end_addr.saturating_sub(1)));

    // Calculate page flags
    let mut page_flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;

    // If the segment is writable, add the WRITABLE flag
    if flags.is_write() {
        page_flags |= PageTableFlags::WRITABLE;
    }

    // If the segment is not executable, add the NO_EXECUTE flag
    if !flags.is_execute() {
        page_flags |= PageTableFlags::NO_EXECUTE;
    }

    // Get HHDM offset for physical-to-virtual translation
    let hhdm = get_hhdm_offset();

    // Map each page in the segment
    for page in Page::range_inclusive(start_page, end_page) {
        // Check if the page is already mapped
        if mapper.translate_page(page).is_ok() {
            // Page already mapped (could be a segment that overlaps previous one)
            // This can happen with segments that share a page
            println!(
                "[ELF]   Page {:#x} already mapped, skipping",
                page.start_address().as_u64()
            );
            continue;
        }

        // Allocate a physical frame
        let frame = frame_allocator
            .allocate_frame()
            .expect("Failed to allocate frame for ELF segment");

        println!(
            "[ELF]   Mapping page {:#x} -> frame {:#x}, flags: {:?}",
            page.start_address().as_u64(),
            frame.start_address().as_u64(),
            page_flags
        );

        // Map the page to the frame
        unsafe {
            mapper
                .map_to(page, frame, page_flags, frame_allocator)
                .expect("Failed to map ELF segment page")
                .flush();
        }

        // Zero out the frame first (important for BSS)
        let frame_virt = frame.start_address().as_u64() + hhdm;
        let frame_ptr = frame_virt as *mut u8;
        unsafe {
            core::ptr::write_bytes(frame_ptr, 0, 4096);
        }
    }

    // Copy the file data to the allocated pages
    if segment_filesz > 0 {
        let file_data = &elf.input[segment_offset..segment_offset + segment_filesz as usize];

        // Copy data page by page
        let mut copied = 0u64;
        let mut current_vaddr = segment_vaddr;

        while copied < segment_filesz {
            let page = Page::<Size4KiB>::containing_address(VirtAddr::new(current_vaddr));
            let page_offset = current_vaddr % 4096;
            let bytes_in_page = core::cmp::min(4096 - page_offset, segment_filesz - copied);

            // Get the physical address of this page through HHDM
            let phys_frame = mapper.translate_page(page).expect("Page should be mapped");
            let dest_ptr = (phys_frame.start_address().as_u64() + hhdm + page_offset) as *mut u8;

            // Copy the data
            unsafe {
                core::ptr::copy_nonoverlapping(
                    file_data[copied as usize..].as_ptr(),
                    dest_ptr,
                    bytes_in_page as usize,
                );
            }

            copied += bytes_in_page;
            current_vaddr += bytes_in_page;
        }
    }
}

/// Process ELF relocations for PIE executables
fn process_relocations(mapper: &mut OffsetPageTable<'static>, elf: &ElfFile, base_addr: u64) {
    use xmas_elf::sections::SectionData;

    let hhdm = get_hhdm_offset();
    let mut total_relocs = 0u64;
    let mut applied_relocs = 0u64;
    let mut skipped_relocs = 0u64;

    // Find .rela.dyn section for relocations
    for section in elf.section_iter() {
        let section_name = section.get_name(elf).unwrap_or("<unknown>");

        if let Ok(SectionData::Rela64(rela_entries)) = section.get_data(elf) {
            println!("[ELF] Processing relocation section: {}", section_name);

            for rela in rela_entries {
                let r_type = rela.get_type();
                let r_offset = rela.get_offset();
                let r_addend = rela.get_addend();

                total_relocs += 1;

                // R_X86_64_RELATIVE = 8
                if r_type == 8 {
                    // Calculate the relocated address
                    let target_vaddr = base_addr + r_offset;
                    // r_addend is i64, base_addr is u64
                    // The relocation value is: base_addr + addend
                    let value = base_addr.wrapping_add(r_addend as u64);

                    // Write the relocated value
                    let page = Page::<Size4KiB>::containing_address(VirtAddr::new(target_vaddr));
                    let page_offset = target_vaddr % 4096;

                    if let Ok(phys_frame) = mapper.translate_page(page) {
                        let dest_ptr =
                            (phys_frame.start_address().as_u64() + hhdm + page_offset) as *mut u64;
                        unsafe {
                            *dest_ptr = value;
                        }
                        applied_relocs += 1;
                    } else {
                        // Page not mapped! This is a problem
                        if skipped_relocs < 5 {
                            println!(
                                "[ELF] WARNING: Reloc target {:#x} not mapped! (offset={:#x}, addend={:#x})",
                                target_vaddr, r_offset, r_addend
                            );
                        }
                        skipped_relocs += 1;
                    }
                } else {
                    // Non-RELATIVE relocation - might need handling
                    if total_relocs <= 5 {
                        println!(
                            "[ELF] Non-RELATIVE reloc type {} at offset {:#x}",
                            r_type, r_offset
                        );
                    }
                }
            }
        }
    }

    println!(
        "[ELF] Relocations: {} total, {} applied, {} skipped",
        total_relocs, applied_relocs, skipped_relocs
    );
}

/// Relocate DYNAMIC segment entries that contain addresses
/// The DYNAMIC segment contains d_tag/d_val pairs. For entries where d_val is an address,
/// we need to add base_addr since these aren't covered by R_X86_64_RELATIVE relocations.
fn relocate_dynamic_segment(mapper: &mut OffsetPageTable<'static>, elf: &ElfFile, base_addr: u64) {
    use xmas_elf::program::Type;

    let hhdm = get_hhdm_offset();

    // Find the DYNAMIC segment
    for ph in elf.program_iter() {
        if let Ok(Type::Dynamic) = ph.get_type() {
            let dyn_vaddr = base_addr + ph.virtual_addr();
            let dyn_size = ph.mem_size() as usize;

            println!(
                "[ELF] Relocating DYNAMIC segment at {:#x}, size {}",
                dyn_vaddr, dyn_size
            );

            // Each dynamic entry is 16 bytes: d_tag (8) + d_val (8)
            let num_entries = dyn_size / 16;
            let mut relocated_count = 0u32;

            for i in 0..num_entries {
                let entry_vaddr = dyn_vaddr + (i as u64 * 16);
                let page = Page::<Size4KiB>::containing_address(VirtAddr::new(entry_vaddr));
                let page_offset = entry_vaddr % 4096;

                if let Ok(phys_frame) = mapper.translate_page(page) {
                    let entry_ptr =
                        (phys_frame.start_address().as_u64() + hhdm + page_offset) as *mut u64;

                    unsafe {
                        let d_tag = *entry_ptr;
                        let d_val_ptr = entry_ptr.add(1);
                        let d_val = *d_val_ptr;

                        // DT_NULL (0) marks end of DYNAMIC section
                        if d_tag == 0 {
                            break;
                        }

                        // Check if this tag's value is an address that needs relocation
                        // These are the common address-type tags:
                        let is_address = matches!(
                            d_tag,
                            3 |     // DT_PLTGOT
                            4 |     // DT_HASH
                            5 |     // DT_STRTAB
                            6 |     // DT_SYMTAB
                            7 |     // DT_RELA
                            12 |    // DT_INIT
                            13 |    // DT_FINI
                            17 |    // DT_REL
                            23 |    // DT_JMPREL
                            25 |    // DT_INIT_ARRAY
                            26 |    // DT_FINI_ARRAY
                            0x6ffffef5 |  // DT_GNU_HASH
                            0x6ffffff0 |  // DT_VERSYM
                            0x6ffffffe |  // DT_VERNEED
                            0x6ffffff9 // DT_RELACOUNT - not an address but we skip it
                        );

                        // Skip DT_RELACOUNT (0x6ffffff9) - it's a count, not an address
                        if d_tag == 0x6ffffff9 {
                            continue;
                        }

                        if is_address && d_val != 0 {
                            // Add base address to the value
                            let new_val = d_val + base_addr;
                            *d_val_ptr = new_val;
                            relocated_count += 1;
                        }
                    }
                }
            }

            println!("[ELF] DYNAMIC: {} entries relocated", relocated_count);
            break; // Only one DYNAMIC segment
        }
    }
}

/// Setup the user stack with proper auxiliary vector
/// Returns the stack pointer (top of stack)
///
/// Stack layout (growing down, addresses decrease):
/// ```
/// (high address - USER_STACK_BOTTOM)
/// ... (zero padding for alignment)
/// AT_NULL, 0            <- auxv end
/// AT_ENTRY, entry_point <- program entry point  
/// AT_PAGESZ, 4096       <- page size
/// AT_PHNUM, phnum       <- number of program headers
/// AT_PHENT, 56          <- size of program header
/// AT_PHDR, phdr_addr    <- address of program headers (allows base calculation)
/// NULL                  <- end of envp
/// NULL                  <- end of argv
/// argc (0)              <- stack pointer points here
/// (low address)
/// ```
pub fn setup_user_stack(
    mapper: &mut OffsetPageTable<'static>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    _hhdm_offset: u64,
) -> VirtAddr {
    let hhdm = get_hhdm_offset();

    // Calculate stack page range
    let stack_start = USER_STACK_BOTTOM - USER_STACK_SIZE;
    let stack_end = USER_STACK_BOTTOM;

    let start_page = Page::<Size4KiB>::containing_address(VirtAddr::new(stack_start));
    let end_page = Page::<Size4KiB>::containing_address(VirtAddr::new(stack_end - 1));

    // Stack is readable, writable, not executable
    let flags = PageTableFlags::PRESENT
        | PageTableFlags::USER_ACCESSIBLE
        | PageTableFlags::WRITABLE
        | PageTableFlags::NO_EXECUTE;

    // Map each stack page
    for page in Page::range_inclusive(start_page, end_page) {
        let frame = frame_allocator
            .allocate_frame()
            .expect("Failed to allocate frame for user stack");

        unsafe {
            mapper
                .map_to(page, frame, flags, frame_allocator)
                .expect("Failed to map user stack page")
                .flush();
        }

        // Zero out the stack frame
        let frame_ptr = (frame.start_address().as_u64() + hhdm) as *mut u8;
        unsafe {
            core::ptr::write_bytes(frame_ptr, 0, 4096);
        }
    }

    // Parse ELF to get phdr info
    let elf = xmas_elf::ElfFile::new(USER_ELF_BYTES).expect("Failed to parse ELF");
    let phdr_offset = elf.header.pt2.ph_offset();
    let phnum = elf.header.pt2.ph_count() as u64;
    let phent = elf.header.pt2.ph_entry_size() as u64;
    let entry = elf.header.pt2.entry_point();

    // Determine base address (same logic as load_user_elf)
    let is_pie = elf.header.pt2.type_().as_type() == xmas_elf::header::Type::SharedObject;
    let base_addr = if is_pie { USER_BASE_ADDR } else { 0 };

    // Calculate relocated addresses
    let phdr_addr = base_addr + phdr_offset;
    let entry_addr = base_addr + entry;

    // Auxiliary vector type constants (from Linux ABI)
    const AT_NULL: u64 = 0;
    const AT_PHDR: u64 = 3;
    const AT_PHENT: u64 = 4;
    const AT_PHNUM: u64 = 5;
    const AT_PAGESZ: u64 = 6;
    const AT_BASE: u64 = 7;
    const AT_ENTRY: u64 = 9;
    const AT_RANDOM: u64 = 25;

    // Stack layout:
    // We need space for: argc, argv[0]=NULL, envp[0]=NULL, auxv entries
    // auxv: 7 entries * 16 bytes = 112 bytes
    // args: 3 * 8 = 24 bytes
    // random bytes: 16 bytes
    // Total: ~160 bytes, round up to 256 for alignment

    let stack_top = USER_STACK_BOTTOM;

    // Reserve space and align to 16 bytes
    let mut sp = stack_top - 256;
    sp = sp & !0xF; // 16-byte alignment

    // Get the physical address for writing
    let write_to_stack = |mapper: &OffsetPageTable, addr: u64, value: u64| {
        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(addr));
        let page_offset = addr % 4096;
        if let Ok(phys_frame) = mapper.translate_page(page) {
            let ptr = (phys_frame.start_address().as_u64() + hhdm + page_offset) as *mut u64;
            unsafe {
                *ptr = value;
            }
        }
    };

    // Build stack from bottom to top (remember: stack grows down, so we place
    // items in order of increasing address)
    let mut offset = 0u64;

    // argc = 0
    write_to_stack(mapper, sp + offset, 0);
    offset += 8;

    // argv[0] = NULL (argv terminator)
    write_to_stack(mapper, sp + offset, 0);
    offset += 8;

    // envp[0] = NULL (envp terminator)
    write_to_stack(mapper, sp + offset, 0);
    offset += 8;

    // Auxiliary vector entries (each is 16 bytes: type, value)
    // AT_PHDR - address of program headers (crucial for base calculation!)
    write_to_stack(mapper, sp + offset, AT_PHDR);
    write_to_stack(mapper, sp + offset + 8, phdr_addr);
    offset += 16;

    // AT_PHENT - size of program header entry
    write_to_stack(mapper, sp + offset, AT_PHENT);
    write_to_stack(mapper, sp + offset + 8, phent);
    offset += 16;

    // AT_PHNUM - number of program headers
    write_to_stack(mapper, sp + offset, AT_PHNUM);
    write_to_stack(mapper, sp + offset + 8, phnum);
    offset += 16;

    // AT_PAGESZ - page size
    write_to_stack(mapper, sp + offset, AT_PAGESZ);
    write_to_stack(mapper, sp + offset + 8, 4096);
    offset += 16;

    // AT_BASE - interpreter base (0 for static, base_addr for PIE)
    write_to_stack(mapper, sp + offset, AT_BASE);
    write_to_stack(mapper, sp + offset + 8, 0); // 0 for static PIE (no interpreter)
    offset += 16;

    // AT_ENTRY - program entry point
    write_to_stack(mapper, sp + offset, AT_ENTRY);
    write_to_stack(mapper, sp + offset + 8, entry_addr);
    offset += 16;

    // AT_RANDOM - pointer to 16 random bytes (we'll just point to a zeros area)
    write_to_stack(mapper, sp + offset, AT_RANDOM);
    write_to_stack(mapper, sp + offset + 8, sp + 240); // Point to reserved area
    offset += 16;

    // AT_NULL - end of auxv
    write_to_stack(mapper, sp + offset, AT_NULL);
    write_to_stack(mapper, sp + offset + 8, 0);

    println!(
        "[STACK] User stack at {:#x}, AT_PHDR={:#x}, AT_ENTRY={:#x}",
        sp, phdr_addr, entry_addr
    );

    VirtAddr::new(sp)
}

/// Enter user mode and jump to the entry point
/// This function never returns
#[inline(never)]
pub unsafe fn enter_userspace(entry_point: VirtAddr, stack_pointer: VirtAddr) -> ! {
    // User Code Segment selector: Ring 3 with RPL 3
    // GDT layout: 0=null, 1=kernel_code, 2=kernel_data, 3=user_data, 4=user_code
    // User data segment at index 3, RPL 3 = (3 << 3) | 3 = 0x1B
    // User code segment at index 4, RPL 3 = (4 << 3) | 3 = 0x23

    println!(
        "[USERSPACE] Jumping to Ring 3 at {:#x}",
        entry_point.as_u64()
    );

    // Use iretq to enter user mode
    // Stack must be set up as: SS, RSP, RFLAGS, CS, RIP (in that order, pushed)

    // Segment selectors
    const USER_DATA_SEL: u64 = 0x1B; // User data segment (GDT index 3, RPL 3)
    const USER_CODE_SEL: u64 = 0x23; // User code segment (GDT index 4, RPL 3)
    const RFLAGS_USER: u64 = 0x202; // IF=1, reserved bit 1=1

    let entry = entry_point.as_u64();
    let stack = stack_pointer.as_u64();

    println!("[DEBUG] Entry: {:#x}, Stack: {:#x}", entry, stack);
    println!(
        "[DEBUG] USER_CODE_SEL: {:#x}, USER_DATA_SEL: {:#x}",
        USER_CODE_SEL, USER_DATA_SEL
    );

    unsafe {
        core::arch::asm!(
            // Build iretq stack frame:
            // iretq expects: RIP, CS, RFLAGS, RSP, SS (from top to bottom)
            // So we push in reverse order: SS, RSP, RFLAGS, CS, RIP

            // Push SS (Stack Segment for Ring 3)
            "push {ss}",
            // Push RSP (User stack pointer)
            "push {rsp_user}",
            // Push RFLAGS
            "push {rflags}",
            // Push CS (Code Segment for Ring 3)
            "push {cs}",
            // Push RIP (Entry point)
            "push {rip}",

            // Set data segment selectors to user mode before iretq
            // (DS, ES need to be set; FS/GS will be set by userspace if needed)
            "mov ax, {ss:x}",
            "mov ds, ax",
            "mov es, ax",

            // Clear general purpose registers for security
            // (except the stack pointer which iretq will load)
            "xor rax, rax",
            "xor rbx, rbx",
            "xor rcx, rcx",
            "xor rdx, rdx",
            "xor rsi, rsi",
            "xor rdi, rdi",
            "xor rbp, rbp",
            "xor r8, r8",
            "xor r9, r9",
            "xor r10, r10",
            "xor r11, r11",
            "xor r12, r12",
            "xor r13, r13",
            "xor r14, r14",
            "xor r15, r15",

            // iretq pops: RIP, CS, RFLAGS, RSP, SS
            "iretq",

            ss = in(reg) USER_DATA_SEL,
            rsp_user = in(reg) stack,
            rflags = in(reg) RFLAGS_USER,
            cs = in(reg) USER_CODE_SEL,
            rip = in(reg) entry,
            options(noreturn)
        );
    }
}
