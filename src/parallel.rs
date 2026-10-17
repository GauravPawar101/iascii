#[cfg(feature = "parallel")]
use rayon::ThreadPool;
#[cfg(feature = "parallel")]
use rayon::prelude::*;

use crate::config::{Config, Quality};
use crate::convert::{CHANNELS, convert_image, validate_and_compute_grid_size};
use crate::error::ImageError;
use crate::grid::Grid;
use crate::sampling::Sampler;

/// Output cells per work item when the grid is split by cells instead of by rows.
///
/// Small enough for rayon to balance wide-but-short grids across threads, large
/// enough that per-item bookkeeping stays negligible.
#[cfg(feature = "parallel")]
const CELL_CHUNK: usize = 512;

/// Converts an RGB pixel buffer into an ASCII/ANSI `Grid` using concurrent processing.
///
/// Work is split by row for [`Quality::Best`] (whose error diffusion is row-local and
/// therefore cannot be split mid-row) and by cell otherwise, so wide-but-short grids —
/// e.g. a 2000×4 banner — parallelize instead of falling back to a single thread.
///
/// If `pool` is `None` or the output does not clear `config.parallel_threshold` cells,
/// processing falls back to the exact sequential path used by `convert_image`.
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

    // 4. Threshold & Pool fallback check. The gate is on total output cells, not rows,
    // so wide/short grids are considered on their real amount of work.
    let Some(pool) = pool else {
        return convert_image(width, height, buf, config);
    };

    let total_cells = grid_width as usize * grid_height as usize;
    if total_cells < config.parallel_threshold {
        return convert_image(width, height, buf, config);
    }

    // 5. Concurrent Grid allocation and filling
    let mut grid = Grid::new(grid_width, grid_height);

    // Precompute the ramp characters and column geometry once, shared (read-only)
    // across all worker threads.
    let ramp_chars: Vec<char> = config.ramp.inner().as_str().chars().collect();
    let sampler = Sampler::new(
        buf,
        width,
        height,
        CHANNELS,
        grid_width,
        grid_height,
        config,
        &ramp_chars,
    );

    let row_width = grid_width as usize;

    pool.install(|| {
        if matches!(config.quality, Quality::Best) {
            // Dithering carries error between neighbouring cells, so rows must stay
            // intact: one work item per row keeps the sequential result exact.
            grid.as_mut_slice()
                .par_chunks_mut(row_width)
                .enumerate()
                .for_each(|(row_idx, row_cells)| sampler.fill_row(row_idx as u32, row_cells));
        } else {
            // No cross-cell state, so any contiguous split is valid — including splits
            // that cut through the middle of a row.
            grid.as_mut_slice()
                .par_chunks_mut(CELL_CHUNK)
                .enumerate()
                .for_each(|(chunk_idx, chunk)| {
                    let start = chunk_idx * CELL_CHUNK;
                    let mut row = (start / row_width) as u32;
                    let mut col = (start % row_width) as u32;
                    let (mut y_start, mut y_end) = sampler.row_geometry(row);
                    let mut diffusion_error = 0.0f32;

                    for cell in chunk.iter_mut() {
                        *cell = sampler.cell(col, y_start, y_end, &mut diffusion_error);

                        col += 1;
                        if col as usize == row_width {
                            col = 0;
                            row += 1;
                            if row < grid_height {
                                let geom = sampler.row_geometry(row);
                                y_start = geom.0;
                                y_end = geom.1;
                            }
                        }
                    }
                });
        }
    });

    Ok(grid)
}
