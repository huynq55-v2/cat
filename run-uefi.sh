#!/usr/bin/env bash
set -e

# ==========================
# CONFIG
# ==========================
TARGET=x86_64-unknown-uefi
BUILD=release

ESP_IMG=esp.img
ESP_SIZE=64   # MB
ESP_DIR=esp

OVMF_CODE=/usr/share/OVMF/OVMF_CODE_4M.fd
OVMF_VARS=/usr/share/OVMF/OVMF_VARS_4M.fd

# ==========================
# BUILD UEFI BOOTLOADER
# ==========================
echo "[*] Building UEFI bootloader..."
cargo +nightly uefi_boot --release

# ==========================
# BUILD KERNEL
# ==========================
echo "[*] Building kernel..."
cargo +nightly kernel --release

EFI_PATH=target/x86_64-unknown-uefi/release/uefi_boot.efi

if [ ! -f "$EFI_PATH" ]; then
    echo "[!] EFI file not found: $EFI_PATH"
    exit 1
fi

echo "[*] Preparing OVMF..."
cp $OVMF_VARS .
cp $OVMF_CODE .

mkdir -p esp/efi/boot
cp target/x86_64-unknown-uefi/release/uefi_boot.efi esp/efi/boot/bootx64.efi
cp target/x86_64-unknown-none/release/kernel esp/kernel

qemu-system-x86_64 -enable-kvm \
    -drive if=pflash,format=raw,readonly=on,file=OVMF_CODE_4M.fd \
    -drive if=pflash,format=raw,readonly=on,file=OVMF_VARS_4M.fd \
    -drive format=raw,file=fat:rw:esp \
    -serial stdio