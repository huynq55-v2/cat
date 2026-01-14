---
name: Rust Debugging
description: Tools and commands for debugging Rust OS kernel, UEFI bootloader, and user-space programs.
---

# Rust Debugging Skill

This skill provides commands and workflows for inspecting binaries and debugging the OS components.

## Prerequisites

- **LLVM Tools**: Ensure `llvm-tools-preview` is installed:
  ```bash
  rustup component add llvm-tools-preview
  ```
- **Rust Object Tools**: Recommended usage via `rust-objdump`, `rust-nm`, or standard `llvm-*` tools found in your Rust toolchain.

## 1. User Space Debugging (Static Musl Binaries)

For user space programs compiled with `x86_64-unknown-linux-musl` and `crt-static`.

### List Symbols (`nm`)
To see all symbols in a binary (with Rust demangling):
```bash
rust-nm <BINARY>
# Or using standard nm (demangle with rustfilt if installed)
nm <BINARY> | rustfilt
```

### Disassemble (`objdump`)
To view the assembly instructions. `rust-objdump` automatically demangles names:
```bash
rust-objdump -d <BINARY>
```
Look for `_start` or `main` to verify the entry point.

### Check ELF Headers (`readobj`)
To verify if it's a valid statically linked ELF:
```bash
rust-readobj -h <BINARY>
```

## 2. Kernel & Bootloader Debugging

### Inspect Initial RAM Disk / UEFI Payload
Use `llvm-objdump` to check the kernel binary structure before it gets packed.

```bash
rust-objdump -h target/x86_64-unknown-none/release/kernel
```

### QEMU Monitor
When QEMU is running:
- Press `Ctrl+Alt+2` to switch to the QEMU Monitor.
- Type `info registers` to see CPU state.
- Type `info mem` to see memory mappings (if supported).
- Press `Ctrl+Alt+1` to return to the display.

## 3. Common Cargo Commands

- **Check Code**: `cargo check`
- **Clippy**: `cargo clippy`
- **Macro Expansion**: `cargo expand` (requires `cargo-expand` crate)
