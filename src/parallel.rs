#[cfg(feature = "parallel")]
use rayon::ThreadPool;
#[cfg(feature = "parallel")]
use rayon::prelude::*;

use crate::config::Config;
use crate::convert::{CHANNELS, convert_image, validate_and_compute_grid_size};
use crate::error::ImageError;
use crate::grid::Grid;
use crate::sampling::fill_row;

/// Converts an RGB pixel buffer into an ASCII/ANSI `Grid` using concurrent row processing.
///
/// If `pool` is `None` or the target row count does not clear `config.parallel_threshold`,
/// processing falls back to sequential execution seamlessly.
#[cfg(feature = "parallel")]
pub fn convert_with_pool(
    width: u32,
    height: u32,
    buf: &[u8],
    config: &Config,
    pool: Option<&ThreadPool>,
) -> Result<Grid, ImageError> {
    // 1-3. Shared validation & target size calculation (kept identical to convert_image
    // so sequential and parallel paths can never silently diverge)
    let (grid_width, grid_height) = validate_and_compute_grid_size(width, height, buf, config)?;

    // 4. Threshold & Pool fallback check
    let Some(pool) = pool else {
        return convert_image(width, height, buf, config);
    };

    if (grid_height as usize) < config.parallel_threshold {
        return convert_image(width, height, buf, config);
    }

    // 5. Concurrent Grid allocation and filling
    let mut grid = Grid::new(grid_width, grid_height);

    // Precompute the ramp's character sequence once for the whole conversion, shared
    // (read-only) across all worker threads, instead of reallocating it per row.
    let ramp_chars: Vec<char> = config.ramp.inner().as_str().chars().collect();

    pool.install(|| {
        grid.as_mut_slice()
            .par_chunks_mut(grid_width as usize)
            .enumerate()
            .for_each(|(row_idx, row_cells)| {
                fill_row(
                    row_idx as u32,
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
            });
    });

    Ok(grid)
}
