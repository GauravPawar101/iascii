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

/// Maps an output column to the source x-range `[x_start, x_end)` it samples.
#[inline]
pub fn map_col_geometry(col: u32, grid_width: u32, img_width: u32) -> (u32, u32) {
    let x_start = (col as u64 * img_width as u64 / grid_width as u64) as u32;
    let x_end = ((((col + 1) as u64 * img_width as u64) / grid_width as u64) as u32)
        .max(x_start + 1)
        .min(img_width);
    (x_start, x_end)
}

/// Maps an output row to the source y-range `[y_start, y_end)` it samples.
#[inline]
pub fn map_row_geometry(row: u32, grid_height: u32, img_height: u32) -> (u32, u32) {
    let y_start = (row as u64 * img_height as u64 / grid_height as u64) as u32;
    let y_end = ((((row + 1) as u64 * img_height as u64) / grid_height as u64) as u32)
        .max(y_start + 1)
        .min(img_height);
    (y_start, y_end)
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
    let (x_start, x_end) = map_col_geometry(col, grid_width, img_width);
    let (y_start, y_end) = map_row_geometry(row, grid_height, img_height);
    (x_start, x_end, y_start, y_end)
}

/// Iterates over all source pixels in a block bounding box and returns the averaged RGB color.
///
/// Walks the buffer one horizontal row segment at a time (`chunks_exact`) with a running
/// byte offset instead of recomputing a full pixel offset per pixel, so the inner loop is
/// branch-free and bounds-checked once per row rather than once per pixel. Out-of-range
/// rows are skipped; since missing pixels contribute nothing to the sums, the result
/// matches the per-pixel fallback-to-black behavior exactly.
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
    if channels == 0 || x_end <= x_start || y_end <= y_start {
        return Rgb::BLACK;
    }

    let mut sum_r: u64 = 0;
    let mut sum_g: u64 = 0;
    let mut sum_b: u64 = 0;

    let row_stride = img_width as usize * channels;
    let seg_start = x_start as usize * channels;
    let seg_len = (x_end - x_start) as usize * channels;

    let mut offset = y_start as usize * row_stride + seg_start;
    for _ in y_start..y_end {
        if let Some(segment) = img_buf.get(offset..offset.saturating_add(seg_len)) {
            for px in segment.chunks_exact(channels) {
                sum_r += px[0] as u64;
                sum_g += px[1] as u64;
                sum_b += px[2] as u64;
            }
        }
        offset += row_stride;
    }

    let count = ((x_end - x_start) as u64) * ((y_end - y_start) as u64);
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

/// Immutable, precomputed conversion context reused across every row and thread.
///
/// Two things that used to be recomputed per cell now happen once:
/// the output column → source x-range table (previously two 64-bit divisions per
/// cell, now one pass per conversion) and the ramp parameters (`is_light`,
/// `max_idx`). Row geometry still costs one division pair per row, which is
/// unavoidable but negligible next to per-cell work.
pub(crate) struct Sampler<'a> {
    img_buf: &'a [u8],
    img_width: u32,
    img_height: u32,
    channels: usize,
    grid_width: u32,
    grid_height: u32,
    config: &'a Config,
    ramp_chars: &'a [char],
    is_light: bool,
    max_idx: usize,
    max_idx_f: f32,
    columns: Vec<(u32, u32)>,
}

impl<'a> Sampler<'a> {
    // Mirrors the long-standing `fill_row` signature the pipelines already pass around.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        img_buf: &'a [u8],
        img_width: u32,
        img_height: u32,
        channels: usize,
        grid_width: u32,
        grid_height: u32,
        config: &'a Config,
        ramp_chars: &'a [char],
    ) -> Self {
        let columns = (0..grid_width)
            .map(|col| map_col_geometry(col, grid_width, img_width))
            .collect();

        let max_idx = ramp_chars.len().saturating_sub(1);

        Self {
            img_buf,
            img_width,
            img_height,
            channels,
            grid_width,
            grid_height,
            config,
            ramp_chars,
            is_light: matches!(config.ramp, RampType::Light(_)),
            max_idx,
            max_idx_f: max_idx as f32,
            columns,
        }
    }

    /// Source y-range sampled by an output row.
    #[inline]
    pub(crate) fn row_geometry(&self, row: u32) -> (u32, u32) {
        map_row_geometry(row, self.grid_height, self.img_height)
    }

    /// Fills one output row. Used by the sequential pipeline and by the row-split
    /// parallel path (`Quality::Best`, whose dithering state is row-local).
    #[inline]
    pub(crate) fn fill_row(&self, row_idx: u32, row_cells: &mut [Cell]) {
        let (y_start, y_end) = self.row_geometry(row_idx);
        let mut diffusion_error = 0.0f32;
        for (col, cell) in row_cells.iter_mut().enumerate() {
            *cell = self.cell(col as u32, y_start, y_end, &mut diffusion_error);
        }
    }

    /// Computes one output cell.
    ///
    /// `diffusion_error` threads `Quality::Best` state between adjacent cells in a
    /// row. For the other tiers it is written but never read, so callers that
    /// process independent cells (e.g. a cell-split parallel path) may pass a
    /// throwaway value.
    #[inline]
    pub(crate) fn cell(
        &self,
        col: u32,
        y_start: u32,
        y_end: u32,
        diffusion_error: &mut f32,
    ) -> Cell {
        let (x_start, x_end) = match self.columns.get(col as usize) {
            Some(&geom) => geom,
            // Defensive: a caller handed more cells than the configured grid width.
            None => map_col_geometry(col, self.grid_width, self.img_width),
        };

        let avg_rgb = average_block(
            self.img_buf,
            self.img_width,
            self.channels,
            x_start,
            x_end,
            y_start,
            y_end,
        );

        let idx = match self.config.quality {
            // Fastest tier: fixed-point luminance, direct ramp lookup, no f32 math.
            Quality::Good => {
                let mut lum = integer_luminance(self.config.luminance_method, avg_rgb);
                if self.is_light {
                    lum = 255 - lum;
                }
                if self.max_idx == 0 {
                    0
                } else {
                    ((lum as usize * self.max_idx) + 127) / 255
                }
            }
            // Balanced tier: perceptual floating-point luminance, direct ramp lookup.
            Quality::Better => {
                let mut lum = self
                    .config
                    .luminance_method
                    .calculate(avg_rgb.r, avg_rgb.g, avg_rgb.b);
                if self.is_light {
                    lum = 1.0 - lum;
                }
                (lum.clamp(0.0, 1.0) * self.max_idx_f).round() as usize
            }
            // Highest tier: perceptual luminance plus error diffusion into the next cell.
            Quality::Best => {
                let mut lum = self
                    .config
                    .luminance_method
                    .calculate(avg_rgb.r, avg_rgb.g, avg_rgb.b);
                if self.is_light {
                    lum = 1.0 - lum;
                }
                let adjusted = (lum + *diffusion_error).clamp(0.0, 1.0);
                let exact = adjusted * self.max_idx_f;
                let idx = exact.round().clamp(0.0, self.max_idx_f) as usize;
                *diffusion_error = if self.max_idx_f > 0.0 {
                    ((exact - idx as f32) / self.max_idx_f).clamp(-1.0, 1.0)
                } else {
                    0.0
                };
                idx
            }
        };

        let ch = self.ramp_chars.get(idx).copied().unwrap_or(' ');
        Cell::new(ch, avg_rgb)
    }
}

/// High-performance row-fill routine called by both sequential and parallel processing pipelines.
///
/// `ramp_chars` is the ramp's character sequence, collected once by the caller (see
/// `convert::convert_image` / `parallel::convert_with_pool`) rather than being rebuilt on
/// every row, since this function runs once per output row.
///
/// The internal pipelines share a precomputed [`Sampler`] across rows and threads; this
/// public entry point builds a throwaway one so callers rolling their own concurrency keep
/// the same signature and results.
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
    let sampler = Sampler::new(
        img_buf,
        img_width,
        img_height,
        channels,
        grid_width,
        grid_height,
        config,
        ramp_chars,
    );
    sampler.fill_row(row_idx, row_cells);
}

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

    #[test]
    fn column_and_row_geometry_match_block_geometry() {
        // The specialized helpers must agree with the combined mapping exactly.
        for col in 0..7u32 {
            for row in 0..5u32 {
                let (xs, xe, ys, ye) = map_block_geometry(col, row, 7, 5, 96, 47);
                assert_eq!(map_col_geometry(col, 7, 96), (xs, xe));
                assert_eq!(map_row_geometry(row, 5, 47), (ys, ye));
            }
        }
    }

    #[test]
    fn average_block_ignores_out_of_range_rows() {
        // Buffer shorter than the requested rows: missing rows contribute nothing.
        let buf = vec![255, 255, 255];
        let avg = average_block(&buf, 1, 3, 0, 1, 0, 4);
        assert_eq!(avg, Rgb::new(63, 63, 63)); // (255 + 0 + 0 + 0) / 4
    }
}
