use crate::error::ImageError;
use crate::grid::{Cell, Grid, Rgb};
use crate::render::ColorDepth;
use std::io::Write as IoWrite;

/// Standard 16-color ANSI palette RGB lookup table.
static ANSI16_PALETTE: [(u8, u8, u8); 16] = [
    (0, 0, 0),       // 0: Black
    (128, 0, 0),     // 1: Red
    (0, 128, 0),     // 2: Green
    (128, 128, 0),   // 3: Yellow
    (0, 0, 128),     // 4: Blue
    (128, 0, 128),   // 5: Magenta
    (0, 128, 128),   // 6: Cyan
    (192, 192, 192), // 7: Light Gray
    (128, 128, 128), // 8: Dark Gray
    (255, 0, 0),     // 9: Bright Red
    (0, 255, 0),     // 10: Bright Green
    (255, 255, 0),   // 11: Bright Yellow
    (0, 0, 255),     // 12: Bright Blue
    (255, 0, 255),   // 13: Bright Magenta
    (0, 255, 255),   // 14: Bright Cyan
    (255, 255, 255), // 15: White
];

/// Tracked active terminal color state to avoid duplicate escape codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveColor {
    Reset,
    TrueColor(Rgb),
    Ansi256(u8),
    Ansi16(u8),
}

/// Converts a 24-bit `Rgb` value to the nearest matching 16-color ANSI index (0..15).
fn rgb_to_ansi16(rgb: Rgb) -> u8 {
    let mut min_dist = u32::MAX;
    let mut best_idx = 0;

    for (i, &(r, g, b)) in ANSI16_PALETTE.iter().enumerate() {
        let dr = rgb.r as i32 - r as i32;
        let dg = rgb.g as i32 - g as i32;
        let db = rgb.b as i32 - b as i32;
        let dist = (dr * dr + dg * dg + db * db) as u32;

        if dist < min_dist {
            min_dist = dist;
            best_idx = i as u8;
        }
    }

    best_idx
}

/// Converts a 24-bit `Rgb` value to the nearest matching 256-color ANSI index (16..255).
fn rgb_to_ansi256(rgb: Rgb) -> u8 {
    static CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

    let closest_level = |val: u8| -> (usize, u8) {
        let mut min_d = u32::MAX;
        let mut best_idx = 0;
        for (i, &l) in CUBE_LEVELS.iter().enumerate() {
            let d = (val as i32 - l as i32).unsigned_abs();
            if d < min_d {
                min_d = d;
                best_idx = i;
            }
        }
        (best_idx, CUBE_LEVELS[best_idx])
    };

    let (r_idx, r_val) = closest_level(rgb.r);
    let (g_idx, g_val) = closest_level(rgb.g);
    let (b_idx, b_val) = closest_level(rgb.b);

    let cube_dist = {
        let dr = rgb.r as i32 - r_val as i32;
        let dg = rgb.g as i32 - g_val as i32;
        let db = rgb.b as i32 - b_val as i32;
        (dr * dr + dg * dg + db * db) as u32
    };

    // Evaluate proximity to grayscale ramp (indices 232..255)
    let gray_avg = ((rgb.r as u32 + rgb.g as u32 + rgb.b as u32) / 3) as u8;
    let gray_idx = if gray_avg < 8 {
        0
    } else if gray_avg > 238 {
        23
    } else {
        ((gray_avg - 8) as f32 / 10.0).round() as u8
    };

    let gray_val = 8 + gray_idx * 10;
    let gray_dist = {
        let dr = rgb.r as i32 - gray_val as i32;
        let dg = rgb.g as i32 - gray_val as i32;
        let db = rgb.b as i32 - gray_val as i32;
        (dr * dr + dg * dg + db * db) as u32
    };

    if gray_dist < cube_dist {
        232 + gray_idx
    } else {
        16 + (36 * r_idx + 6 * g_idx + b_idx) as u8
    }
}

/// Appends `value` to `output` in decimal.
///
/// Deliberately avoids the generic `fmt` machinery: every ANSI escape carries one to
/// three integers, and `write!("{}")` per integer was the single largest cost in ANSI
/// rendering. This writes the digits directly into the string buffer.
#[inline]
fn push_decimal(output: &mut String, value: u32) {
    if value < 10 {
        output.push((b'0' + value as u8) as char);
        return;
    }

    let mut digits = [0u8; 10];
    let mut cursor = digits.len();
    let mut remaining = value;
    while remaining > 0 {
        cursor -= 1;
        digits[cursor] = b'0' + (remaining % 10) as u8;
        remaining /= 10;
    }

    // Every byte written above is an ASCII digit, hence valid UTF-8.
    output.push_str(std::str::from_utf8(&digits[cursor..]).expect("ASCII digits"));
}

/// Upper bound on rendered bytes for a depth, so the batch path allocates once.
///
/// The previous estimate (12 bytes per cell) under-counted 19-byte truecolor escapes
/// and `len_utf8` glyphs, forcing reallocation partway through large renders.
fn ansi_capacity(grid: &Grid, depth: ColorDepth) -> usize {
    let escape_bytes = match depth {
        ColorDepth::TrueColor => 19, // "\x1b[38;2;255;255;255m"
        ColorDepth::Ansi256 => 11,   // "\x1b[38;5;255m"
        ColorDepth::Ansi16 => 5,     // "\x1b[97m"
    };
    let cells = grid.width() as usize * grid.height() as usize;
    let glyph_bytes: usize = grid.as_slice().iter().map(|cell| cell.ch.len_utf8()).sum();
    glyph_bytes + cells * escape_bytes + grid.height() as usize * 5 // "\x1b[0m\n" per row
}

/// Appends one ANSI-rendered row (with its trailing reset + newline) to `output`.
///
/// Color-state tracking is scoped to the row because every row ends with a reset,
/// so each row can be rendered independently — which is also what lets the
/// streaming path emit rows without buffering the whole grid.
fn write_ansi_row(row: &[Cell], depth: ColorDepth, output: &mut String) {
    let mut current_color = ActiveColor::Reset;

    for cell in row {
        let target_color = match depth {
            ColorDepth::TrueColor => ActiveColor::TrueColor(cell.color),
            ColorDepth::Ansi256 => ActiveColor::Ansi256(rgb_to_ansi256(cell.color)),
            ColorDepth::Ansi16 => ActiveColor::Ansi16(rgb_to_ansi16(cell.color)),
        };

        // Only emit escape sequences when color state actually changes
        if target_color != current_color {
            match target_color {
                ActiveColor::TrueColor(c) => {
                    output.push_str("\x1b[38;2;");
                    push_decimal(output, c.r as u32);
                    output.push(';');
                    push_decimal(output, c.g as u32);
                    output.push(';');
                    push_decimal(output, c.b as u32);
                    output.push('m');
                }
                ActiveColor::Ansi256(code) => {
                    output.push_str("\x1b[38;5;");
                    push_decimal(output, code as u32);
                    output.push('m');
                }
                ActiveColor::Ansi16(code) => {
                    let escape_code = if code < 8 { 30 + code } else { 90 + (code - 8) };
                    output.push_str("\x1b[");
                    push_decimal(output, escape_code as u32);
                    output.push('m');
                }
                ActiveColor::Reset => {
                    output.push_str("\x1b[0m");
                }
            }
            current_color = target_color;
        }

        output.push(cell.ch);
    }

    // Reset terminal color state at the end of each row
    output.push_str("\x1b[0m\n");
}

/// Converts a completed `Grid` into an ANSI-formatted `String` (batch rendering).
///
/// For large grids, prefer [`render_ansi_to`], which streams to a writer instead
/// of building the full escaped string in memory.
pub fn render_ansi(grid: &Grid, depth: ColorDepth) -> String {
    let mut output = String::with_capacity(ansi_capacity(grid, depth));
    for row in grid.rows() {
        write_ansi_row(row, depth, &mut output);
    }
    output
}

/// Streaming counterpart to [`render_ansi`]: writes escaped rows to `out` as they
/// are produced, holding only one row of scratch space at a time.
pub fn render_ansi_to<W: IoWrite + ?Sized>(
    grid: &Grid,
    depth: ColorDepth,
    out: &mut W,
) -> Result<(), ImageError> {
    // Worst case per cell is a 4-byte glyph plus a 19-byte truecolor escape; the
    // scratch buffer is reused across rows, so it is allocated once.
    let mut scratch = String::with_capacity(grid.width() as usize * 24 + 8);
    for row in grid.rows() {
        scratch.clear();
        write_ansi_row(row, depth, &mut scratch);
        out.write_all(scratch.as_bytes())?;
    }
    Ok(())
}
