use crate::config::{Config, LuminanceMethod, Quality};
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
///
/// Walks the buffer one horizontal row segment at a time (`chunks_exact`) instead of
/// recomputing a full pixel offset per pixel, which keeps the inner loop branch-free and
/// bounds-checked once per row rather than once per pixel. Out-of-range rows are skipped;
/// since missing pixels contribute nothing to the sums, the result matches the
/// per-pixel fallback-to-black behavior exactly.
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

    let row_stride = img_width as usize * channels;
    let seg_start = x_start as usize * channels;
    let seg_end = x_end as usize * channels;

    for py in y_start..y_end {
        let row_offset = py as usize * row_stride;
        let Some(segment) = img_buf.get(row_offset + seg_start..row_offset + seg_end) else {
            continue;
        };
        for px in segment.chunks_exact(channels) {
            sum_r += px[0] as u64;
            sum_g += px[1] as u64;
            sum_b += px[2] as u64;
        }
    }

    let count = ((x_end - x_start) as u64) * ((y_end - y_start) as u64);
    if count == 0 {
        return Rgb::BLACK;
    }

    Rgb::new(
        (sum_r / count) as u8,
        (sum_g / count) as u8,
        (sum_b / count) as u8,
    )
}

/// Fixed-point luminance in `0..=255`, matching the perceptual weights of the
/// floating-point methods without any `f32` math (used by [`Quality::Good`]).
#[inline]
fn integer_luminance(method: LuminanceMethod, rgb: Rgb) -> u32 {
    let r = rgb.r as u32;
    let g = rgb.g as u32;
    let b = rgb.b as u32;
    match method {
        LuminanceMethod::Average => (r + g + b) / 3,
        // 0.299/0.587/0.114 scaled by 256 (weights sum to 256).
        LuminanceMethod::Bt601 => (77 * r + 150 * g + 29 * b) >> 8,
        // 0.2126/0.7152/0.0722 scaled by 256 (weights sum to 256).
        LuminanceMethod::Bt709 => (54 * r + 184 * g + 18 * b) >> 8,
    }
}

/// High-performance row-fill routine called by both sequential and parallel processing pipelines.
///
/// `ramp_chars` is the ramp's character sequence, collected once by the caller (see
/// `convert::convert_image` / `parallel::convert_with_pool`) rather than being rebuilt on
/// every row, since this function runs once per output row.
#[allow(clippy::too_many_arguments)]
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
    let max_idx = ramp_chars.len().saturating_sub(1);
    let max_idx_f = max_idx as f32;

    // Quality::Best carries quantization error from one cell to the next within this row
    // (horizontal Floyd–Steinberg-style diffusion). It is deliberately row-local: rows stay
    // independent, so sequential and `parallel` conversions remain byte-identical.
    let mut diffusion_error = 0.0f32;

    for col in 0..grid_width {
        let (x_start, x_end, y_start, y_end) =
            map_block_geometry(col, row_idx, grid_width, grid_height, img_width, img_height);

        let avg_rgb = average_block(img_buf, img_width, channels, x_start, x_end, y_start, y_end);

        let idx = match config.quality {
            // Fastest tier: fixed-point luminance, direct ramp lookup, no f32 math.
            Quality::Good => {
                let mut lum = integer_luminance(config.luminance_method, avg_rgb);
                if is_light {
                    lum = 255 - lum;
                }
                if max_idx == 0 {
                    0
                } else {
                    ((lum as usize * max_idx) + 127) / 255
                }
            }
            // Balanced tier: perceptual floating-point luminance, direct ramp lookup.
            Quality::Better => {
                let mut lum = config
                    .luminance_method
                    .calculate(avg_rgb.r, avg_rgb.g, avg_rgb.b);
                if is_light {
                    lum = 1.0 - lum;
                }
                (lum.clamp(0.0, 1.0) * max_idx_f).round() as usize
            }
            // Highest tier: perceptual luminance plus error diffusion into the next cell.
            Quality::Best => {
                let mut lum = config
                    .luminance_method
                    .calculate(avg_rgb.r, avg_rgb.g, avg_rgb.b);
                if is_light {
                    lum = 1.0 - lum;
                }
                let adjusted = (lum + diffusion_error).clamp(0.0, 1.0);
                let exact = adjusted * max_idx_f;
                let idx = exact.round().clamp(0.0, max_idx_f) as usize;
                diffusion_error = if max_idx_f > 0.0 {
                    ((exact - idx as f32) / max_idx_f).clamp(-1.0, 1.0)
                } else {
                    0.0
                };
                idx
            }
        };

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
