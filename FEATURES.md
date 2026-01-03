# OS Features and Status

This document tracks the **actual** implemented features of the OS based on the codebase analysis.

## 1. Bootloader (UEFI)
- [x] **Custom UEFI Bootloader**:
    - Implemented in `uefi_boot` using `uefi` crate.
    - **ELF Loading**: Parses and loads 64-bit ELF kernel file.
    - **Memory Setup**:
        - Allocates and sets up PML4 Page Table.
        - **HHDM**: Maps physical memory to Higher Half (`0xffff_8000_0000_0000`).
        - **Kernel Mapping**: Maps kernel segments and stack (20KB) to Higher Half.
        - **Trampoline**: Identity maps current execution code for safe context switching.
    - **Handover**: Exits Boot Services and jumps to Kernel Entry with `mmap` info.

## 2. Kernel Core
- [x] **Entry Point**: `_start` function receives Memory Map, HHDM offset, etc.
- [x] **No-std Support**:
    - Custom `panic_handler` (in `shared` library).
- [x] **Hardware Abstraction**:
    - **GDT & TSS**: Initialized in `src/gdt.rs` (Code Segment, TSS with Double Fault Stack).
    - **Serial Output**: Debug output via Serial Port 0x3F8 (in `shared`).
- [ ] **Interrupts (IDT)**: Not yet implemented.
- [ ] **VGA Text Mode**: Not yet implemented (only Serial output available).

## 3. Memory Management
- [x] **Physical Memory (PMM)**:
    - `pmm::init`: Initializes Physical Memory Manager (likely Bitmap/Spinlock based).
    - `KernelFrameAllocator`: Implements `FrameAllocator` trait.
- [x] **Virtual Memory**:
    - `pml4::init_mapper`: Creates `OffsetPageTable` using HHDM offset.
- [x] **Heap Allocation**:
    - `linked_list_allocator` initialized in `src/heap_allocator.rs`.
    - Supports `alloc` types (`Box`, `Vec`, etc.).

## 4. Current Functionality
- Kernel boots successfully via UEFI.
- Initializes Memory (Paging, Frame Allocator, Heap).
- Initializes GDT/TSS.
- Prints debug info to Serial Port.
- Verifies Heap with `Vec` push test.
- Enters infinite `hlt` loop.

---

## Roadmap / Next Steps

### Immediate Priorities
- [ ] **IDT (Interrupt Descriptor Table)**: Implement Exception Handlers (Page Fault, General Protection, etc.) and Hardware Interrupts (Timer, Keyboard).
- [ ] **VGA / Framebuffer Driver**: Implement a screen driver (GOP-based Framebuffer or VGA Text Mode) for visual output.

### Future
- [ ] **Multitasking**: Implement Task struct, Context Switching, and Scheduler.
- [ ] **Userspace**: Implement User Mode switching and Syscalls.
- [ ] **Filesystem**: VFS and Loading files from disk.
