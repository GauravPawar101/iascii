use crate::config::Config;
use crate::grid::{Cell, Rgb};
use crate::ramp::RampType;

/// Reads the RGB pixel value at (x, y) from a raw flat byte buffer.
#[inline]
pub fn get_pixel_rgb(img_buf: &[u8], img_width: u32, x: u32, y: u32, channels: usize) -> Rgb {
    let offset = ((y * img_width + x) as usize) * channels;

    if offset + 2 < img_buf.len() {
        Rgb::new(img_buf[offset], img_buf[offset + 1], img_buf[offset + 2])
    } else {
        Rgb::BLACK
    }
}

/// Maps a target grid cell (col, row) to its corresponding source pixel bounding box (x_start, x_end, y_start, y_end).
#[inline]
pub fn map_block_geometry(
    col: u32,
    row: u32,
    grid_width: u32,
    grid_height: u32,
    img_width: u32,
    img_height: u32,
) -> (u32, u32, u32, u32) {
    let x_start = (col as u64 * img_width as u64 / grid_width as u64) as u32;
    let mut x_end = (((col + 1) as u64 * img_width as u64) / grid_width as u64) as u32;
    x_end = x_end.max(x_start + 1).min(img_width);

    let y_start = (row as u64 * img_height as u64 / grid_height as u64) as u32;
    let mut y_end = (((row + 1) as u64 * img_height as u64) / grid_height as u64) as u32;
    y_end = y_end.max(y_start + 1).min(img_height);

    (x_start, x_end, y_start, y_end)
}

/// Iterates over all source pixels in a block bounding box and returns the averaged RGB color.
#[inline]
pub fn average_block(
    img_buf: &[u8],
    img_width: u32,
    channels: usize,
    x_start: u32,
    x_end: u32,
    y_start: u32,
    y_end: u32,
) -> Rgb {
    let mut sum_r: u64 = 0;
    let mut sum_g: u64 = 0;
    let mut sum_b: u64 = 0;

    for py in y_start..y_end {
        for px in x_start..x_end {
            let pixel = get_pixel_rgb(img_buf, img_width, px, py, channels);
            sum_r += pixel.r as u64;
            sum_g += pixel.g as u64;
            sum_b += pixel.b as u64;
        }
    }

    let count = ((x_end - x_start) * (y_end - y_start)) as u64;
    if count == 0 {
        return Rgb::BLACK;
    }

    Rgb::new(
        (sum_r / count) as u8,
        (sum_g / count) as u8,
        (sum_b / count) as u8,
    )
}

/// High-performance row-fill routine called by both sequential and parallel processing pipelines.
///
/// `ramp_chars` is the ramp's character sequence, collected once by the caller (see
/// `convert::convert_image` / `parallel::convert_with_pool`) rather than being rebuilt on
/// every row, since this function runs once per output row.
pub fn fill_row(
    row_idx: u32,
    row_cells: &mut [Cell],
    img_buf: &[u8],
    img_width: u32,
    img_height: u32,
    channels: usize,
    grid_width: u32,
    grid_height: u32,
    config: &Config,
    ramp_chars: &[char],
) {
    // ramp_chars is normally guaranteed non-empty by Config::build()'s validation, but
    // fill_row is a public function that can be called directly (e.g. by callers rolling
    // their own concurrency instead of using `parallel::convert_with_pool`), so it must not
    // assume that validation ran. saturating_sub avoids an underflow panic on an empty ramp;
    // if ramp_chars ends up empty, every cell just falls back to a space.
    let is_light = matches!(config.ramp, RampType::Light(_));
    let max_idx = ramp_chars.len().saturating_sub(1) as f32;

    for col in 0..grid_width {
        let (x_start, x_end, y_start, y_end) =
            map_block_geometry(col, row_idx, grid_width, grid_height, img_width, img_height);

        let avg_rgb = average_block(img_buf, img_width, channels, x_start, x_end, y_start, y_end);

        let mut lum = config
            .luminance_method
            .calculate(avg_rgb.r, avg_rgb.g, avg_rgb.b);

        if is_light {
            lum = 1.0 - lum;
        }

        let lum = lum.clamp(0.0, 1.0);
        let idx = (lum * max_idx).round() as usize;
        let ch = ramp_chars.get(idx).copied().unwrap_or(' ');

        if let Some(cell) = row_cells.get_mut(col as usize) {
            *cell = Cell::new(ch, avg_rgb);
        }
    }
}

// Place at the bottom of src/sampling.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_and_block_sampling() {
        let img_buf = vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255];

        let oob = get_pixel_rgb(&img_buf, 2, 5, 5, 3);
        assert_eq!(oob, Rgb::BLACK);

        let (x_start, x_end, y_start, y_end) = map_block_geometry(0, 0, 1, 1, 2, 2);
        assert_eq!((x_start, x_end, y_start, y_end), (0, 2, 0, 2));

        let avg = average_block(&img_buf, 2, 3, x_start, x_end, y_start, y_end);
        assert_eq!(avg, Rgb::new(127, 127, 127));
    }
}
