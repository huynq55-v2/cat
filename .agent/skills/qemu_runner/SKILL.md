---
name: QEMU Runner
description: Identify environment and run the OS kernel in QEMU with serial output.
---

# QEMU Runner Skill

This skill provides a unified way to launch the OS in QEMU across different platforms and capture serial logs.

## 1. Environment Detection

Before running, identify the shell environment:

### PowerShell (Windows)
```powershell
if ($env:OS -like "*Windows*") {
    Write-Host "Running on Windows"
    .\run-uefi.ps1
}
```

### Bash (Linux/WSL)
```bash
if [[ "$OSTYPE" == "linux-gnu"* ]] || [[ "$OSTYPE" == "msys" ]]; then
    echo "Running on Linux/MSYS"
    chmod +x run-uefi.sh
    ./run-uefi.sh
fi
```

## 2. Running with Serial Logging (Timestamped)

To run the kernel and capture logs to a unique file for each run:

### PowerShell
```powershell
$timestamp = Get-Date -Format "yyyyMMdd_HHmmss"
$logFile = "serial_logs/$timestamp.txt"
Write-Host "Logging to $logFile"
& ./run-uefi.ps1 | Tee-Object -FilePath $logFile
```

### Bash
```bash
timestamp=$(date +%Y%m%d_%H%M%S)
log_file="serial_logs/$timestamp.txt"
echo "Logging to $log_file"
./run-uefi.sh | tee "$log_file"
```

## 3. Exiting QEMU

QEMU usually stays open after the kernel finishes. To exit:
- **Manual**: Click the QEMU window and press `Ctrl+C` in the terminal to kill the process.
- **Serial Mode**: If using `-display none`, press `Ctrl+A` then `X` to exit.
- **Auto-Close**: Update your `run-uefi.ps1` or `.sh` script to include `-no-reboot` to prevent infinite loops on panic.

## 4. QEMU Serial Parameters
Ensure the QEMU command includes:
- `-serial stdio`: Directs memory logs to the terminal.
- `-display none`: (Optional) CLI-only mode, no VGA window.
- `-no-reboot`: Exit/Stop on failure instead of rebooting.

## 5. Troubleshooting
- **Windows**: If `run-uefi.ps1` fails with execution policy errors, run `Set-ExecutionPolicy -ExecutionPolicy RemoteSigned -Scope Process`.
- **Linux**: Ensure QEMU and OVMF are installed (`sudo apt install qemu-system-x86 ovmf`).
