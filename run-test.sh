#!/usr/bin/env bash
set -e

# ==========================
# CONFIG
# ==========================
OVMF_CODE=/usr/share/OVMF/OVMF_CODE_4M.fd
OVMF_VARS=/usr/share/OVMF/OVMF_VARS_4M.fd

# ==========================
# BUILD UEFI BOOTLOADER
# ==========================
echo "[*] Building UEFI bootloader..."
cargo +nightly uefi_boot --release

# ==========================
# BUILD KERNEL (TEST MODE)
# ==========================
echo "[*] Building kernel (integration-test mode)..."
# We build using normal 'build' but with the 'integration-test' feature
# This bypasses the tricky 'cargo test' no_std compilation issues.
cargo +nightly kernel --release --features integration-test

# Find the binary
# Since we are using standard build, the path is deterministic
TEST_BINARY="target/x86_64-unknown-none/release/kernel"

if [ ! -f "$TEST_BINARY" ]; then
    echo "[!] Could not find compiled kernel binary at $TEST_BINARY!"
    exit 1
fi

echo "[*] Found test binary: $TEST_BINARY"

# ==========================
# PREPARE EFI IMAGE
# ==========================
mkdir -p esp/efi/boot
cp target/x86_64-unknown-uefi/release/uefi_boot.efi esp/efi/boot/bootx64.efi
cp "$TEST_BINARY" esp/kernel

# ==========================
# RUN QEMU TEST
# ==========================
echo "[*] Running QEMU Test..."

# Exit Status Logic:
# 0x10 (Success) -> (0x10 << 1) | 1 = 33
# 0x11 (Failed)  -> (0x11 << 1) | 1 = 35

set +e # Disable exit-on-error for QEMU command because we expect non-zero exit codes

qemu-system-x86_64 \
    -enable-kvm \
    -display none \
    -m 512M \
    -drive if=pflash,format=raw,readonly=on,file=$OVMF_CODE \
    -drive if=pflash,format=raw,readonly=on,file=$OVMF_VARS \
    -drive format=raw,file=fat:rw:esp \
    -serial stdio \
    -device isa-debug-exit,iobase=0xf4,iosize=0x04

EXIT_CODE=$?

if [ $EXIT_CODE -eq 33 ]; then
    echo -e "\n[+] TESTS PASSED (QEMU exit code 33)"
    exit 0
elif [ $EXIT_CODE -eq 35 ]; then
    echo -e "\n[-] TESTS FAILED (QEMU exit code 35)"
    exit 1
else
    echo -e "\n[!] QEMU Exited Unexpectedly with code $EXIT_CODE"
    exit 1
fi
