use crate::grid::{Grid, Rgb};
use crate::render::ColorDepth;
use std::fmt::Write;

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

/// Converts a completed `Grid` into an ANSI-formatted string.
pub fn render_ansi(grid: &Grid, depth: ColorDepth) -> String {
    let capacity = (grid.height() as usize) * ((grid.width() as usize) * 12 + 8);
    let mut output = String::with_capacity(capacity);
    let mut current_color = ActiveColor::Reset;

    for row in grid.rows() {
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
                        let _ = write!(output, "\x1b[38;2;{};{};{}m", c.r, c.g, c.b);
                    }
                    ActiveColor::Ansi256(code) => {
                        let _ = write!(output, "\x1b[38;5;{code}m");
                    }
                    ActiveColor::Ansi16(code) => {
                        let escape_code = if code < 8 { 30 + code } else { 90 + (code - 8) };
                        let _ = write!(output, "\x1b[{escape_code}m");
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
        current_color = ActiveColor::Reset;
    }

    output
}
