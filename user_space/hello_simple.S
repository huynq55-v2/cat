# Simple Hello Program using Raw Syscalls
# Compile with: nasm -f elf64 hello_simple.asm -o hello_simple.o && ld hello_simple.o -o hello_simple

.global _start
.section .data
    msg: .ascii "[USER SPACE] Hello World!\n"
    msg_len = . - msg

.section .text
_start:
    # sys_write(1, msg, len)
    mov $1, %rax          # syscall number for write
    mov $1, %rdi          # file descriptor (stdout)
    lea msg(%rip), %rsi   # pointer to message
    mov $msg_len, %rdx    # length of message
    syscall

    # sys_exit(0)
    mov $60, %rax         # syscall number for exit
    xor %rdi, %rdi        # exit code 0
    syscall
