#![no_std] // This crate does not use the standard library

// Module for serial port communication
pub mod serial;

// Module for panic handling
pub mod panic;

// Module for helper functions
pub mod helpers;

pub mod framebuffer;

#[repr(C)]
pub struct BootInfo {
    pub memory_map_addr: u64,
    pub memory_map_len: u64,
    pub memory_map_desc_size: u64,
    pub hhdm_offset: u64,
    pub max_phys_memory: u64,
    pub framebuffer: framebuffer::FrameBufferInfo,
}
