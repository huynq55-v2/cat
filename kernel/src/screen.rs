use core::fmt;
use core::ptr;
use font8x8::{BASIC_FONTS, UnicodeFonts};
use lazy_static::lazy_static;
use shared::framebuffer::{FrameBufferInfo, PixelFormat};
use spin::Mutex;

const FONT_WIDTH: usize = 8;
const FONT_HEIGHT: usize = 8;

pub struct FrameBufferWriter {
    info: FrameBufferInfo,
    x_pos: usize,
    y_pos: usize,
    scale: usize,
    text_color: u32,
    bg_color: u32,
}

impl FrameBufferWriter {
    pub fn set_scale(&mut self, scale: usize) {
        self.scale = scale;
    }

    pub fn set_text_color(&mut self, color: u32) {
        self.text_color = self.convert_color(color);
    }

    pub fn set_background_color(&mut self, color: u32) {
        self.bg_color = self.convert_color(color);
    }

    fn convert_color(&self, color: u32) -> u32 {
        let r = (color >> 16) & 0xFF;
        let g = (color >> 8) & 0xFF;
        let b = color & 0xFF;

        match self.info.format {
            // If the screen is RGB (Byte 0 is Red): Need to write to RAM as RR GG BB
            // Due to Little Endian structure, the integer must be 0x00BBGGRR
            PixelFormat::RGB => (b << 16) | (g << 8) | r,

            // If the screen is BGR (Byte 0 is Blue): Need to write to RAM as BB GG RR
            // Due to Little Endian structure, the integer must be 0x00RRGGBB (keep input)
            PixelFormat::BGR => color,

            // Any other cases
            _ => color,
        }
    }

    pub fn clear(&mut self, color: u32) {
        let buffer = self.info.buffer_base as *mut u32;
        unsafe {
            for y in 0..self.info.height {
                let row_start = buffer.add(y * self.info.stride);
                for x in 0..self.info.width {
                    *row_start.add(x) = self.convert_color(color);
                }
            }
        }
        self.x_pos = 0;
        self.y_pos = 0;
    }

    pub fn write_byte(&mut self, byte: u8) {
        let scaled_width = FONT_WIDTH * self.scale;
        match byte {
            b'\n' => self.fill_remainder(),

            byte => {
                if self.x_pos + scaled_width > self.info.width {
                    self.new_line();
                }
                self.draw_char(self.x_pos, self.y_pos, byte as char);
                self.x_pos += scaled_width;
            }
        }
    }

    fn new_line(&mut self) {
        let scaled_height = FONT_HEIGHT * self.scale;
        self.x_pos = 0;
        self.y_pos += scaled_height;

        if self.y_pos + scaled_height > self.info.height {
            self.scroll_up();
            self.y_pos -= scaled_height;
        }
    }

    fn scroll_up(&mut self) {
        let scaled_height = FONT_HEIGHT * self.scale;
        let stride = self.info.stride;
        let height = self.info.height;
        let buffer = self.info.buffer_base as *mut u32;

        unsafe {
            // Copy the entire screen up
            let copy_lines = height - scaled_height;
            let copy_len = copy_lines * stride;
            let src = buffer.add(stride * scaled_height);
            let dst = buffer;
            ptr::copy(src, dst, copy_len);

            let last_line_start = buffer.add(copy_len);
            let fill_len = scaled_height * stride;

            for i in 0..fill_len {
                *last_line_start.add(i) = self.bg_color;
            }
        }
    }

    fn draw_char(&mut self, x: usize, y: usize, c: char) {
        let bitmap = match BASIC_FONTS.get(c) {
            Some(glyph) => glyph,
            None => return,
        };

        let buffer = self.info.buffer_base as *mut u32;
        let stride = self.info.stride;

        for (row_idx, &row_byte) in bitmap.iter().enumerate() {
            for col_idx in 0..8 {
                let bit_is_set = (row_byte & (1 << col_idx)) != 0;

                let pixel_color = if bit_is_set {
                    self.text_color
                } else {
                    self.bg_color
                };

                for sy in 0..self.scale {
                    for sx in 0..self.scale {
                        let dx = x + (col_idx * self.scale) + sx;
                        let dy = y + (row_idx * self.scale) + sy;

                        if dx < self.info.width && dy < self.info.height {
                            unsafe {
                                *buffer.add(dy * stride + dx) = pixel_color;
                            }
                        }
                    }
                }
            }
        }
    }

    fn fill_remainder(&mut self) {
        let scaled_height = FONT_HEIGHT * self.scale;
        let buffer = self.info.buffer_base as *mut u32;
        let stride = self.info.stride;

        if self.x_pos < self.info.width {
            unsafe {
                for y_offset in 0..scaled_height {
                    let dy = self.y_pos + y_offset;
                    if dy >= self.info.height {
                        break;
                    }

                    let row_start = buffer.add(dy * stride);

                    for x in self.x_pos..self.info.width {
                        *row_start.add(x) = self.bg_color;
                    }
                }
            }
        }
        self.new_line();
    }
}

impl fmt::Write for FrameBufferWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for c in s.chars() {
            self.write_byte(c as u8);
        }
        Ok(())
    }
}

lazy_static! {
    pub static ref WRITER: Mutex<Option<FrameBufferWriter>> = Mutex::new(None);
}

pub fn init(info: FrameBufferInfo) {
    let mut writer = WRITER.lock();
    *writer = Some(FrameBufferWriter {
        info,
        x_pos: 0,
        y_pos: 0,
        scale: 2,
        text_color: 0xFFFFFF,
        bg_color: 0x0000FF,
    });
}

// ==========================================
// HELPER FUNCTIONS
// ==========================================

// Set text color for the next print
pub fn set_text_color(color: u32) {
    // Auto lock and set color, user doesn't need to worry about Mutex
    if let Some(writer) = &mut *WRITER.lock() {
        writer.set_text_color(color);
    }
}

// Set background color for the next print
pub fn set_background_color(color: u32) {
    if let Some(writer) = &mut *WRITER.lock() {
        writer.set_background_color(color);
    }
}

// Set font scale for the next print
pub fn set_scale(scale: usize) {
    if let Some(writer) = &mut *WRITER.lock() {
        writer.set_scale(scale);
    }
}

// Clear the screen with the specified background color
pub fn clear_screen(color: u32) {
    if let Some(writer) = &mut *WRITER.lock() {
        writer.clear(color);
    }
}

// Reset to default (White text, Scale 2, Blue background)
pub fn reset_style() {
    if let Some(writer) = &mut *WRITER.lock() {
        writer.set_text_color(0xFFFFFF);
        writer.set_scale(2);
        writer.set_background_color(0x0000FF);
    }
}

// Print a single character
pub fn print_char(c: char) {
    if let Some(writer) = &mut *WRITER.lock() {
        writer.write_byte(c as u8);
    }
}
