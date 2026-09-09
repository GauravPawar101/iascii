use crate::config::{Config, OutputSizing};
use crate::error::ImageError;
use crate::grid::Grid;
use crate::sampling::fill_row;

/// Number of bytes per pixel (RGB8 format).
pub(crate) const CHANNELS: usize = 3;

/// Shared input-validation and target-size calculation used by both the sequential
/// (`convert_image`) and parallel (`parallel::convert_with_pool`) pipelines.
///
/// Centralizing this here means the two pipelines cannot silently drift apart on what
/// counts as a valid input or how output dimensions are derived, which matters for the
/// "sequential and parallel produce identical output" guarantee.
pub(crate) fn validate_and_compute_grid_size(
    width: u32,
    height: u32,
    buf: &[u8],
    config: &Config,
) -> Result<(u32, u32), ImageError> {
    // 1. Dimension validation
    if width == 0 || height == 0 {
        return Err(ImageError::ZeroSizedImage(width as usize, height as usize));
    }

    // 2. Buffer validation
    let expected_len = (width as usize) * (height as usize) * CHANNELS;
    if buf.len() != expected_len {
        return Err(ImageError::BufferSizeMismatch(expected_len, buf.len()));
    }

    // 3. Target size calculation
    let (grid_width, grid_height) = match config.output_sizing {
        OutputSizing::Explicit {
            width: w,
            height: h,
        } => (w as u32, h as u32),
        OutputSizing::MaxWidth(max_w) => {
            let gw = max_w as u32;
            let scale = gw as f32 / width as f32;
            let gh = ((height as f32) * scale * config.aspect_ratio_correction).round() as u32;
            (gw, gh)
        }
    };

    if grid_width == 0 || grid_height == 0 {
        return Err(ImageError::ZeroSizedImage(
            grid_width as usize,
            grid_height as usize,
        ));
    }

    Ok((grid_width, grid_height))
}

/// Converts a raw RGB pixel buffer into an ASCII/ANSI `Grid` based on the provided `Config`.
pub fn convert_image(
    width: u32,
    height: u32,
    buf: &[u8],
    config: &Config,
) -> Result<Grid, ImageError> {
    // 1-3. Shared validation & target size calculation
    let (grid_width, grid_height) = validate_and_compute_grid_size(width, height, buf, config)?;

    // 4. Grid allocation
    let mut grid = Grid::new(grid_width, grid_height);

    // 5. Precompute the ramp's character sequence once for the whole conversion, rather
    // than reallocating it inside fill_row on every single row.
    let ramp_chars: Vec<char> = config.ramp.inner().as_str().chars().collect();

    // 6. Row-by-row filling
    for row_idx in 0..grid_height {
        if let Some(row_cells) = grid.row_mut(row_idx) {
            fill_row(
                row_idx,
                row_cells,
                buf,
                width,
                height,
                CHANNELS,
                grid_width,
                grid_height,
                config,
                &ramp_chars,
            );
        }
    }

    // 7. Return populated grid
    Ok(grid)
}
