#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub enum PixelFormat {
    RGB,
    BGR,
    U8,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct FrameBufferInfo {
    pub buffer_base: u64,
    pub buffer_size: usize,
    pub width: usize,
    pub height: usize,
    pub stride: usize,
    pub format: PixelFormat,
}
