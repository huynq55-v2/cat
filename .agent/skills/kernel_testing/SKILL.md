---
name: Kernel Testing
description: Automated testing workflow for the Kernel using QEMU and Rust's custom test frameworks.
---

# Kernel Testing Skill

This skill allows you to run unit and integration tests for the kernel, verifying functionality on QEMU.

## 1. Running Tests

To run the kernel test suite:

```bash
chmod +x run-test.sh
./run-test.sh
```

## 2. How it Works

1.  **Compilation**: The kernel is compiled in `test` mode, which includes the `custom_test_frameworks` harness and a `test_main` entry point.
2.  **UEFI Boot**: The test kernel is packaged into a UEFI bootable image, similar to the main OS.
3.  **QEMU Execution**: The scripts launches QEMU with a special `isa-debug-exit` device.
4.  **Reporting**:
    *   Tests results are printed to the Serial Port.
    *   Pass: QEMU exits with status `33` ((0x10 << 1) | 1).
    *   Fail/Panic: QEMU exits with status `35` ((0x11 << 1) | 1).
    *   The `run-test.sh` script captures this exit code and reports `PASS` or `FAIL`.

## 3. Adding New Tests

Add `#[test_case]` to any function in the kernel to register it as a test.

```rust
#[cfg(test)]
#[test_case]
fn trivial_assertion() {
    assert_eq!(1, 1);
}
```

## 4. Troubleshooting
- If QEMU hangs, check if the `isa-debug-exit` device is correctly configured in `run-test.sh`.
- Ensure `test_main()` is conditionally called in `_start` inside `kernel/src/main.rs`.
