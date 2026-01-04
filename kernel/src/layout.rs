// Define the virtual address where the heap starts
// We place it in the higher half memory to keep it separate from user space
pub const HEAP_START: u64 = 0xFFFF_9000_0000_0000;

// Define the size of the heap (100 KB)
pub const HEAP_SIZE: usize = 100 * 1024;
