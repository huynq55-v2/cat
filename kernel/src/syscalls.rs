// Syscall Handler Module
// This module implements system call handling for user space programs
// It uses the SYSCALL/SYSRET mechanism on x86_64

use core::arch::naked_asm;
use x86_64::VirtAddr;
use x86_64::registers::model_specific::{Efer, EferFlags, LStar, SFMask, Star};
use x86_64::registers::rflags::RFlags;

// Syscall numbers (Linux x86_64 ABI)
const SYS_READ: u64 = 0;
const SYS_WRITE: u64 = 1;
const SYS_CLOSE: u64 = 3;
const SYS_FSTAT: u64 = 5;
const SYS_POLL: u64 = 7; // Fixed: was 23
const SYS_MMAP: u64 = 9;
const SYS_MPROTECT: u64 = 10;
const SYS_MUNMAP: u64 = 11;
const SYS_BRK: u64 = 12;
const SYS_RT_SIGACTION: u64 = 13;
const SYS_RT_SIGPROCMASK: u64 = 14;
const SYS_IOCTL: u64 = 16;
const SYS_PREAD64: u64 = 17;
const SYS_PWRITE64: u64 = 18;
const SYS_WRITEV: u64 = 20;
const SYS_MADVISE: u64 = 28;
const SYS_FUTEX: u64 = 202;
const SYS_CLOCK_GETTIME: u64 = 228;
const SYS_EXIT: u64 = 60;
const SYS_EXIT_GROUP: u64 = 231;
const SYS_ARCH_PRCTL: u64 = 158;
const SYS_SET_TID_ADDRESS: u64 = 218;
const SYS_SIGALTSTACK: u64 = 131;
const SYS_GETRANDOM: u64 = 318;

// ARCH_PRCTL sub-functions
const ARCH_SET_FS: u64 = 0x1002;
const ARCH_GET_FS: u64 = 0x1003;
const ARCH_SET_GS: u64 = 0x1001;
const ARCH_GET_GS: u64 = 0x1004;

// File descriptors
const STDOUT: u64 = 1;
const STDERR: u64 = 2;

// Simple brk implementation - current break address
static mut CURRENT_BRK: u64 = 0x800_0000; // 128 MB initial break

/// Initialize the syscall mechanism
/// This sets up SYSCALL/SYSRET for handling system calls from user space
pub unsafe fn init(hhdm_offset: u64) {
    // Initialize ELF loader with HHDM offset
    crate::elf_loader::init_hhdm(hhdm_offset);

    // Initialize kernel syscall stack
    init_syscall_stack();

    // Enable System Call Extensions (SCE) in EFER
    unsafe {
        let efer = Efer::read();
        Efer::write(efer | EferFlags::SYSTEM_CALL_EXTENSIONS);

        // Setup STAR register:
        // For SYSRET in 64-bit mode:
        //   User CS = STAR[63:48] + 16
        //   User SS = STAR[63:48] + 8
        // For SYSCALL:
        //   Kernel CS = STAR[47:32]
        //   Kernel SS = STAR[47:32] + 8
        //
        // Our GDT layout:
        //   0x08: Kernel Code
        //   0x10: Kernel Data
        //   0x18: User Data (index 3)
        //   0x20: User Code (index 4)
        //
        // For SYSCALL: kernel CS = 0x08, kernel SS = 0x10
        // For SYSRET: user CS = 0x10 + 16 = 0x20 (correct!), user SS = 0x10 + 8 = 0x18 (correct!)
        let sysret_base: u16 = 0x10 | 3; // SYSRET base with RPL 3
        let syscall_base: u16 = 0x08; // SYSCALL base (kernel CS)

        Star::write_raw(sysret_base, syscall_base);

        // Setup LSTAR - syscall entry point
        LStar::write(VirtAddr::new(syscall_entry as *const () as u64));

        // Setup SFMASK - flags to clear on syscall
        // Clear IF (interrupt flag) and TF (trap flag) on syscall entry
        SFMask::write(RFlags::INTERRUPT_FLAG | RFlags::TRAP_FLAG);
    }

    println!("[SYSCALL] Handler initialized");
}

/// Syscall entry point (naked function)
/// Called when userspace executes SYSCALL instruction
///
/// On entry:
///   - RCX = User RIP (return address)
///   - R11 = User RFLAGS
///   - RAX = Syscall number
///   - RDI, RSI, RDX, R10, R8, R9 = Arguments 1-6
///
/// On exit (SYSRET):
///   - RCX = User RIP
///   - R11 = User RFLAGS
///   - RAX = Return value
#[unsafe(naked)]
extern "C" fn syscall_entry() {
    naked_asm!(
        // Save user stack pointer to a temporary location
        // We cannot use a register like R12 because we must preserve it for the user
        "mov [rip + TEMP_RSP], rsp",

        // Switch to kernel stack
        "lea rsp, [rip + KERNEL_SYSCALL_STACK_TOP]",
        "mov rsp, [rsp]",

        // Push saved User Stack Pointer
        "push qword ptr [rip + TEMP_RSP]",

        // Save User RIP (RCX) and RFLAGS (R11)
        "push rcx",
        "push r11",

        // Save registers that must be preserved + arguments
        // Linux ABI requires preserving: RBX, RBP, R12-R15
        // And arguments: RDI, RSI, RDX, R10, R8, R9
        "push rdi",
        "push rsi",
        "push rdx",
        "push r10",
        "push r8",
        "push r9",

        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // Note: RAX is not saved because it holds the syscall number and returns the result

        // Prepare arguments for Rust handler:
        // syscall_handler_inner(nr, arg1, arg2, arg3, arg4, arg5, arg6)
        // Rust uses: RDI, RSI, RDX, RCX, R8, R9

        // Stack layout (top to bottom):
        // R15, R14, R13, R12, RBP, RBX, R9, R8, R10, RDX, RSI, RDI, R11, RCX, RSP
        // Offsets:
        // 0: R15
        // ...
        // 40: RBX
        // 48: R9 (arg6)
        // 56: R8 (arg5)
        // 64: R10 (arg4)
        // 72: RDX (arg3)
        // 80: RSI (arg2)
        // 88: RDI (arg1)

        // Setup Rust arguments
        // nr (RAX) -> RDI
        // arg1 (RDI) -> RSI
        // arg2 (RSI) -> RDX
        // arg3 (RDX) -> RCX
        // arg4 (R10) -> R8
        // arg5 (R8)  -> R9
        // arg6 (R9)  -> stack

        "mov rdi, rax",         // nr
        "mov rsi, [rsp + 88]",  // arg1
        "mov rdx, [rsp + 80]",  // arg2
        "mov rcx, [rsp + 72]",  // arg3
        "mov r8,  [rsp + 64]",  // arg4 (saved R10)
        "mov r9,  [rsp + 56]",  // arg5 (saved R8)

        "push qword ptr [rsp + 48]", // arg6 (saved R9) -> stack

        "call {handler}",

        "add rsp, 8",           // Cleanup arg6

        // Restore registers
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",

        "pop r9",
        "pop r8",
        "pop r10",
        "pop rdx",
        "pop rsi",
        "pop rdi",

        // Restore SYSCALL/SYSRET context
        "pop r11",       // User RFLAGS
        "pop rcx",       // User RIP
        "pop rsp",       // User RSP

        // Return to userspace
        "sysretq",

        handler = sym syscall_handler_inner,
    );
}

#[unsafe(no_mangle)]
static mut TEMP_RSP: u64 = 0;

// Kernel syscall stack (16 KB)
#[repr(C, align(16))]
struct KernelSyscallStack([u8; 16384]);

#[unsafe(no_mangle)]
static mut KERNEL_SYSCALL_STACK: KernelSyscallStack = KernelSyscallStack([0; 16384]);

#[unsafe(no_mangle)]
static mut KERNEL_SYSCALL_STACK_TOP: u64 = 0;

/// Initialize kernel syscall stack
fn init_syscall_stack() {
    unsafe {
        let stack_ptr = core::ptr::addr_of!(KERNEL_SYSCALL_STACK) as u64;
        KERNEL_SYSCALL_STACK_TOP = stack_ptr + 16384 - 8; // Align and leave space
    }
}

/// Main syscall handler (called from assembly)
#[unsafe(no_mangle)]
extern "C" fn syscall_handler_inner(
    nr: u64,   // Syscall number
    arg1: u64, // Arg1
    arg2: u64, // Arg2
    arg3: u64, // Arg3
    arg4: u64, // Arg4
    arg5: u64, // Arg5
    arg6: u64, // Arg6 (from stack)
) -> i64 {
    handle_syscall(nr, arg1, arg2, arg3, arg4, arg5, arg6)
}

/// Actual syscall handling logic
fn handle_syscall(
    nr: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    _arg5: u64,
    _arg6: u64,
) -> i64 {
    // Debug: log all syscalls
    shared::serial::_print(format_args!(
        "[SC] nr={} a1={:#x} a2={:#x} a3={:#x}\n",
        nr, arg1, arg2, arg3
    ));

    let result = match nr {
        SYS_WRITE => sys_write(arg1, arg2, arg3),
        SYS_READ => sys_read(arg1, arg2, arg3),
        SYS_EXIT => sys_exit(arg1),
        SYS_EXIT_GROUP => sys_exit_group(arg1),
        SYS_BRK => sys_brk(arg1),
        SYS_MMAP => sys_mmap(arg1, arg2, arg3, arg4),
        SYS_MPROTECT => sys_mprotect(arg1, arg2, arg3),
        SYS_MUNMAP => sys_munmap(arg1, arg2),
        SYS_ARCH_PRCTL => sys_arch_prctl(arg1, arg2),
        SYS_SET_TID_ADDRESS => sys_set_tid_address(arg1),
        SYS_POLL => sys_poll(arg1, arg2, arg3),
        SYS_RT_SIGACTION => sys_rt_sigaction(arg1, arg2, arg3),
        SYS_RT_SIGPROCMASK => sys_rt_sigprocmask(arg1, arg2, arg3, arg4),
        SYS_SIGALTSTACK => sys_sigaltstack(arg1, arg2),
        SYS_GETRANDOM => sys_getrandom(arg1, arg2, arg3),
        SYS_FSTAT => sys_fstat(arg1, arg2),
        SYS_IOCTL => sys_ioctl(arg1, arg2, arg3),
        SYS_WRITEV => sys_writev(arg1, arg2, arg3),
        SYS_MADVISE => 0, // Ignore madvise
        _ => {
            println!("[SYSCALL] Unhandled syscall: {}", nr);
            -38 // ENOSYS
        }
    };

    shared::serial::_print(format_args!("[SC] -> {}\n", result));
    result
}

/// SYS_WRITE - Write to file descriptor
fn sys_write(fd: u64, buf: u64, count: u64) -> i64 {
    // Only handle stdout (1) and stderr (2)
    if fd != STDOUT && fd != STDERR {
        return -9; // EBADF
    }

    // Safety: we trust the user pointer for now
    // In a real kernel, we would validate this
    let slice = unsafe { core::slice::from_raw_parts(buf as *const u8, count as usize) };

    // Convert to string and print
    if let Ok(s) = core::str::from_utf8(slice) {
        // Print without adding newline
        for c in s.chars() {
            crate::screen::print_char(c);
        }
        // Also print to serial
        shared::serial::_print(format_args!("{}", s));
    } else {
        // Print raw bytes as characters
        for &byte in slice {
            crate::screen::print_char(byte as char);
        }
    }

    count as i64
}

/// SYS_READ - Read from file descriptor
fn sys_read(_fd: u64, _buf: u64, _count: u64) -> i64 {
    // Not implemented - return 0 (EOF)
    0
}

/// SYS_EXIT - Exit process
fn sys_exit(status: u64) -> i64 {
    println!("\n[KERNEL] User process exited with status: {}", status);

    // Halt the system (for now, we just loop)
    loop {
        x86_64::instructions::hlt();
    }
}

/// SYS_EXIT_GROUP - Exit all threads
fn sys_exit_group(status: u64) -> i64 {
    sys_exit(status)
}

/// SYS_BRK - Change data segment size
fn sys_brk(addr: u64) -> i64 {
    unsafe {
        if addr == 0 {
            // Return current break
            return CURRENT_BRK as i64;
        }

        // Simple implementation: just update the break
        // In a real kernel, we would allocate/deallocate pages
        if addr >= 0x800_0000 && addr < 0x1_0000_0000 {
            // Allow brk within reasonable range (128MB to 4GB)
            CURRENT_BRK = addr;
            addr as i64
        } else if addr < CURRENT_BRK {
            // Allow shrinking
            CURRENT_BRK = addr;
            addr as i64
        } else {
            // For larger allocations, just accept them for now
            CURRENT_BRK = addr;
            addr as i64
        }
    }
}

/// SYS_MMAP - Map memory
/// NOTE: This is a simple implementation that returns addresses from a pre-allocated pool.
/// For musl static PIE, we use addresses that should be in the already-loaded ELF's BSS
/// or we return addresses from a range we'll pre-map.
fn sys_mmap(addr: u64, length: u64, _prot: u64, flags: u64) -> i64 {
    // Flags bit 0x20 = MAP_ANONYMOUS
    let _is_anon = (flags & 0x20) != 0;

    // Use memory starting after the ELF's data segment
    // ELF loads at 0x400000, data ends around 0x470320,
    // we use region starting at 0x480000 (which is mapped as part of user space)
    static mut MMAP_NEXT: u64 = 0x480000;
    static mut MMAP_END: u64 = 0x500000; // Must match MMAP_POOL_END in elf_loader

    unsafe {
        let aligned_len = (length + 0xFFF) & !0xFFF; // Page align

        let result = if addr != 0 && addr >= 0x1000 {
            // Fixed mapping requested
            addr
        } else {
            // Allocate from our pool
            if MMAP_NEXT + aligned_len > MMAP_END {
                // Out of memory
                return -12; // ENOMEM
            }
            let alloc_addr = MMAP_NEXT;
            MMAP_NEXT += aligned_len;
            alloc_addr
        };

        // Note: Ideally we should map these pages here, but we don't have access
        // to the page mapper in syscall context. This works if the pages are
        // pre-mapped by the ELF loader for the BSS/heap region.

        result as i64
    }
}

/// SYS_MPROTECT - Change memory protection
fn sys_mprotect(_addr: u64, _len: u64, _prot: u64) -> i64 {
    // Stub: pretend it worked
    0
}

/// SYS_MUNMAP - Unmap memory
fn sys_munmap(_addr: u64, _len: u64) -> i64 {
    // Stub: pretend it worked
    0
}

/// SYS_ARCH_PRCTL - Architecture-specific thread control
fn sys_arch_prctl(code: u64, addr: u64) -> i64 {
    match code {
        ARCH_SET_FS => {
            // Set FS base for TLS
            x86_64::registers::model_specific::FsBase::write(VirtAddr::new(addr));
            0
        }
        ARCH_GET_FS => {
            // Get FS base
            let fs = x86_64::registers::model_specific::FsBase::read();
            unsafe {
                *(addr as *mut u64) = fs.as_u64();
            }
            0
        }
        ARCH_SET_GS => {
            x86_64::registers::model_specific::GsBase::write(VirtAddr::new(addr));
            0
        }
        ARCH_GET_GS => {
            let gs = x86_64::registers::model_specific::GsBase::read();
            unsafe {
                *(addr as *mut u64) = gs.as_u64();
            }
            0
        }
        _ => -22, // EINVAL
    }
}

/// SYS_SET_TID_ADDRESS - Set pointer to thread ID
fn sys_set_tid_address(_tidptr: u64) -> i64 {
    // Return a fake TID
    1
}

/// SYS_POLL - Poll file descriptors
fn sys_poll(_fds: u64, _nfds: u64, _timeout: u64) -> i64 {
    // Return 0 (timeout with no events)
    0
}

/// SYS_RT_SIGACTION - Set signal action
/// Signature: rt_sigaction(signum, act, oldact, sigsetsize)
fn sys_rt_sigaction(_signum: u64, _act: u64, oldact: u64) -> i64 {
    // If oldact is provided, zero it out to indicate no previous handler
    // kernel sigaction structure on x86_64 is 32 bytes:
    // - sa_handler: 8 bytes
    // - sa_flags: 8 bytes
    // - sa_restorer: 8 bytes
    // - sa_mask: 8 bytes
    if oldact != 0 {
        unsafe {
            core::ptr::write_bytes(oldact as *mut u8, 0, 32);
        }
    }
    0
}

/// SYS_RT_SIGPROCMASK - Change signal mask
/// Signature: rt_sigprocmask(how, set, oldset, sigsetsize)
fn sys_rt_sigprocmask(_how: u64, _set: u64, oldset: u64, _sigsetsize: u64) -> i64 {
    // If oldset is provided, zero it out to indicate empty mask
    // kernel sigset_t is 8 bytes on x86_64
    if oldset != 0 {
        unsafe {
            core::ptr::write_bytes(oldset as *mut u8, 0, 8);
        }
    }
    0
}

/// SYS_SIGALTSTACK - Set/get signal stack
/// Signature: sigaltstack(ss, old_ss)
fn sys_sigaltstack(_ss: u64, old_ss: u64) -> i64 {
    // If old_ss is provided, fill it with "no alternate stack" info
    // stack_t structure: { void *ss_sp; int ss_flags; size_t ss_size; }
    // Total 24 bytes on x86_64
    if old_ss != 0 {
        unsafe {
            let ptr = old_ss as *mut u64;
            // ss_sp = NULL
            *ptr = 0;
            // ss_flags = SS_DISABLE (2)
            *(ptr.add(1) as *mut i32) = 2;
            // ss_size = 0
            *ptr.add(2) = 0;
        }
    }
    0
}

/// SYS_GETRANDOM - Get random bytes
fn sys_getrandom(buf: u64, buflen: u64, _flags: u64) -> i64 {
    // Simple pseudo-random implementation
    // In a real kernel, use a proper RNG
    let slice = unsafe { core::slice::from_raw_parts_mut(buf as *mut u8, buflen as usize) };

    // Use a simple LFSR or just timestamp-based pseudo-random
    static mut SEED: u64 = 0x12345678DEADBEEF;

    for byte in slice.iter_mut() {
        unsafe {
            SEED = SEED.wrapping_mul(6364136223846793005).wrapping_add(1);
            *byte = (SEED >> 33) as u8;
        }
    }

    buflen as i64
}

/// SYS_FSTAT - Get file status
fn sys_fstat(fd: u64, statbuf: u64) -> i64 {
    // Fill in a minimal stat structure for stdin/stdout/stderr
    if fd <= STDERR {
        // Zero out the stat buffer (144 bytes on Linux x86_64)
        let buf = statbuf as *mut u8;
        unsafe {
            core::ptr::write_bytes(buf, 0, 144);
            // Set st_mode to indicate character device (S_IFCHR = 0o020000)
            // offset 24 in stat64
            let mode_ptr = buf.add(24) as *mut u32;
            *mode_ptr = 0o020000 | 0o666; // char device, rw-rw-rw-
        }
        0
    } else {
        -9 // EBADF
    }
}

/// SYS_IOCTL - I/O control
fn sys_ioctl(_fd: u64, _request: u64, _arg: u64) -> i64 {
    // Stub: return ENOTTY for most ioctls
    -25 // ENOTTY
}

/// SYS_WRITEV - Write vector
fn sys_writev(fd: u64, iov: u64, iovcnt: u64) -> i64 {
    // iovec structure: { void *iov_base; size_t iov_len; }
    let mut total = 0i64;

    for i in 0..iovcnt {
        let iovec_ptr = (iov + i * 16) as *const u64;
        unsafe {
            let base = *iovec_ptr;
            let len = *iovec_ptr.add(1);

            let result = sys_write(fd, base, len);
            if result < 0 {
                return result;
            }
            total += result;
        }
    }

    total
}
