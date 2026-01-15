---
name: QEMU Runner
description: Identify environment and run the OS kernel in QEMU with serial output.
---

# QEMU Runner Skill

This skill provides a unified way to launch the OS in QEMU on Linux and capture serial logs.

## 1. Environment Detection

Before running, ensure the shell environment is Linux:

### Bash (Linux)
```bash
if [[ "$OSTYPE" == "linux-gnu"* ]]; then
    echo "Running on Linux"
    chmod +x run-uefi.sh
    ./run-uefi.sh
else
    echo "Error: This script only supports Linux."
    exit 1
fi
```

## 2. Running with Serial Logging (Timestamped)

To run the kernel and capture logs to a unique file for each run:

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
- **Auto-Close**: Update your `run-uefi.sh` script to include `-no-reboot` to prevent infinite loops on panic.

## 4. QEMU Serial Parameters
Ensure the QEMU command includes:
- `-serial stdio`: Directs memory logs to the terminal.
- `-display none`: (Optional) CLI-only mode, no VGA window.
- `-no-reboot`: Exit/Stop on failure instead of rebooting.

## 5. Troubleshooting
- **Linux**: Ensure QEMU and OVMF are installed (`sudo apt install qemu-system-x86 ovmf`).

