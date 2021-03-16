use volatile::Volatile;
use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::instructions::interrupts::without_interrupts;

lazy_static! {
    pub static ref CONSOLE: Mutex<Console> = Mutex::new(Console {
        column_position: 0,
        color_code: ColorCode::new(Color::Yellow, Color::Black),
        buffer: unsafe { &mut *(0xb8000 as *mut Buffer) },
    });
}

const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum  Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct ColorCode(u8);

impl ColorCode {
    fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct Char {
    ascii_character: u8,
    color_code: ColorCode
}

#[repr(transparent)]
struct Buffer {
    chars: [[Volatile<Char>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

pub struct Console {
    column_position: u8,
    color_code: ColorCode,
    buffer: &'static mut Buffer
}

impl Console {

    pub fn write_char(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.column_position >= BUFFER_WIDTH as u8 {
                    self.new_line();
                }

                let row = BUFFER_HEIGHT - 1;
                let col = self.column_position as usize;

                let color_code = self.color_code;
                self.buffer.chars[row][col].write(Char {
                    ascii_character: byte,
                    color_code,
                });
                self.column_position += 1;
            }
        }
    }

    pub fn write_line(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => self.write_char(byte),
                _ => self.write_char(0xfe),
            }
        }
    }

    fn new_line(&mut self) {
        for i in 1 .. BUFFER_HEIGHT  {
            for j in 0 .. BUFFER_WIDTH {
                let char = self.buffer.chars[i][j].read();
                self.buffer.chars[i - 1][j].write(char);
            }
        }
        self.clear_row(BUFFER_HEIGHT - 1);
        self.column_position = 0;
    }

    fn clear_row(&mut self, row: usize) {
        let blank = Char {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }
    pub fn clear(&mut self) {
        for row in 1 .. BUFFER_HEIGHT {
            for col in 0 .. BUFFER_WIDTH {
                let blank = Char {
                    ascii_character: b' ',
                    color_code: self.color_code,
                };
                self.buffer.chars[row][col].write(blank);
            }
        }
        self.column_position = 0;
    }
}

impl fmt::Write for Console {

    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_line(s);
        Ok(())
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga::buffer::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    without_interrupts(|| {
        CONSOLE.lock().write_fmt(args).unwrap();
    })
}