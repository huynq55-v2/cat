// Vùng HHDM bắt đầu
pub const HHDM_OFFSET: u64 = 0xFFFF_8000_0000_0000;

// Vùng Heap bắt đầu (Cách HHDM 1TB)
pub const HEAP_START: u64 = 0xFFFF_9000_0000_0000;
pub const HEAP_SIZE: usize = 100 * 1024; // 100 KiB ban đầu, có thể mở rộng sau

// Vùng Kernel Code
pub const KERNEL_BASE: u64 = 0xFFFF_FFFF_8000_0000;
