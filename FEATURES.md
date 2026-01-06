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
- [x] **Interrupts (IDT)**: Implemented using `x86_interrupt` ABI. Handles Exceptions (PF, GP) and Hardware Interrupts (Timer, Keyboard).
- [x] **VGA / Framebuffer**: Implemented software text rendering on UEFI Framebuffer (Graphics Output Protocol).

## 3. Memory Management
- [x] **Physical Memory (PMM)**:
    - `pmm::init`: Initializes Physical Memory Manager (likely Bitmap/Spinlock based).
    - `KernelFrameAllocator`: Implements `FrameAllocator` trait.
- [x] **Virtual Memory**:
    - `pml4::init_mapper`: Creates `OffsetPageTable` using HHDM offset.
- [x] **Heap Allocation**:
    - `linked_list_allocator` initialized in `src/heap_allocator.rs`.
    - Supports `alloc` types (`Box`, `Vec`, etc.).

## 4. User Space & System Calls
- [x] **ELF Loader**:
    - Loads ELF64 binaries (PIE & Static).
    - **Auxiliary Vector (AuxV)**: Provides AT_PHDR, AT_ENTRY, AT_RANDOM, etc. for glibc/musl support.
    - **Relocations**: Supports user-mode self-relocation for PIE executables.
- [x] **Ring 3 Transition**:
    - `enter_userspace` using `iretq`.
    - Proper GDT/TSS setup for user code/data segments.
- [x] **System Call Interface**:
    - `syscall` / `sysret` instruction support.
    - **Linux ABI Compatibility**: Full register preservation (RDI, RSI, RDX, R10, R8, R9, R12-R15).
    - **Implemented Syscalls**:
        - `write`: Console output (stdout/stderr).
        - `arch_prctl`: FS/GS base setting (TLS support).
        - `brk`, `mmap`: Basic memory allocation (Heap/Mmap pool pre-mapped).
        - `set_tid_address`, `exit_group`.
        - `poll`, `rt_sigaction`, `rt_sigprocmask`, `sigaltstack` (Stubs with correct struct handling).
- [x] **Libc Support**:
    - Verified support for **Musl libc** (Rust binary running in userspace).

## 5. Current Functionality
- Kernel boots successfully via UEFI.
- Initializes Memory (Paging, Frame Allocator, Heap).
- Initializes GDT/TSS and IDT (Interrupts).
- **Runs User Space Program**: Successfully loads and executes a Rust + Musl "Hello World" PIE binary.
- **Syscall Handling**: Handles syscalls from userspace and returns results.

---

## Roadmap / Next Steps

### Immediate Priorities
- [ ] **Memory Management Improvement**: 
    - Implement real `mmap` backing (allocate frames on demand) instead of pre-mapping.
    - Implement real `brk` resizing.
- [ ] **Multitasking**:
    - Implement `fork` / `clone` syscalls.
    - Simple Round-Robin Scheduler.

### Future
- [ ] **Filesystem**: VFS and Loading files from disk.
- [ ] **Shell**: A simple shell to run programs.
