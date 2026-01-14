# run-uefi.ps1
# PowerShell script to run the project in QEMU on Windows

$ErrorActionPreference = "Stop"

# ==========================
# CONFIG
# ==========================
$MSYS2_ROOT = "C:\msys64"
$UCRT64_BIN = "$MSYS2_ROOT\ucrt64\bin"
$QEMU_PATH = "$UCRT64_BIN\qemu-system-x86_64.exe"
$OVMF_CODE = "$MSYS2_ROOT\ucrt64\share\qemu\edk2-x86_64-code.fd"
# If you don't have a vars file, you can often skip it or use a default one
$OVMF_VARS = "$MSYS2_ROOT\ucrt64\share\qemu\edk2-i386-vars.fd" 

$ESP_DIR = "esp"
$EFI_BOOT_DIR = "$ESP_DIR\efi\boot"

# Check if QEMU exists
if (-not (Test-Path $QEMU_PATH)) {
    Write-Host "[!] QEMU not found at $QEMU_PATH" -ForegroundColor Red
    Write-Host "[*] Make sure you have installed mingw-w64-ucrt-x86_64-qemu via pacman."
    exit 1
}

# ==========================
# BUILD
# ==========================
Write-Host "[*] Building UEFI bootloader..." -ForegroundColor Cyan
cargo +nightly build -p uefi_boot --target x86_64-unknown-uefi --release

Write-Host "[*] Building kernel..." -ForegroundColor Cyan
cargo +nightly build -p kernel --target x86_64-unknown-none --release

# ==========================
# PREPARE ESP
# ==========================
Write-Host "[*] Preparing ESP directory..." -ForegroundColor Cyan
if (-not (Test-Path $EFI_BOOT_DIR)) {
    New-Item -ItemType Directory -Force -Path $EFI_BOOT_DIR | Out-Null
}

$EFI_SOURCE = "target/x86_64-unknown-uefi/release/uefi_boot.efi"
$KERNEL_SOURCE = "target/x86_64-unknown-none/release/kernel"

if (-not (Test-Path $EFI_SOURCE)) {
    Write-Host "[!] EFI file not found: $EFI_SOURCE" -ForegroundColor Red
    exit 1
}

Copy-Item $EFI_SOURCE -Destination "$EFI_BOOT_DIR\bootx64.efi" -Force
if (Test-Path $KERNEL_SOURCE) {
    Copy-Item $KERNEL_SOURCE -Destination "$ESP_DIR\kernel" -Force
}

# ==========================
# RUN QEMU
# ==========================
Write-Host "[*] Launching QEMU..." -ForegroundColor Green

# Note: -accel whpx requires Windows Hypervisor Platform features to be enabled.
# If it fails, you can try -accel tcg (slower emulation).
$ACCEL = "tcg" 

& $QEMU_PATH `
    -accel $ACCEL `
    -m 512M `
    -drive "if=pflash,format=raw,readonly=on,file=$OVMF_CODE" `
    -drive "if=pflash,format=raw,readonly=on,file=$OVMF_VARS" `
    -drive "format=raw,file=fat:rw:$ESP_DIR" `
    -serial stdio
