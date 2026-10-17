use crate::error::ImageError;
use crate::grid::{Cell, Grid};
use std::io::Write;

/// Appends one row of plain glyphs plus a terminating newline to `output`.
#[inline]
fn write_ascii_row(row: &[Cell], output: &mut String) {
    // `extend` over a `char` iterator encodes straight into the reserved buffer,
    // which is measurably cheaper than a `push` per cell with repeated capacity checks.
    output.extend(row.iter().map(|cell| cell.ch));
    output.push('\n');
}

/// Exact byte length of the plain rendering, including trailing newlines.
///
/// Summing `len_utf8` costs one cheap pass over the cells but lets [`render_ascii`]
/// allocate precisely once, even for multi-byte ramps (emoji, block shading) where a
/// naive `width * height` guess would trigger several reallocations.
fn ascii_body_len(grid: &Grid) -> usize {
    let glyphs: usize = grid.as_slice().iter().map(|cell| cell.ch.len_utf8()).sum();
    glyphs + grid.height() as usize
}

/// Renders a `Grid` as plain text: one glyph per cell, one `\n` per row, and no
/// escape sequences whatsoever.
///
/// This is the "batch" ASCII entry point — it returns a ready-to-use `String`.
/// For output too large to hold in memory, use [`render_ascii_to`], which streams
/// the same bytes to any writer.
pub fn render_ascii(grid: &Grid) -> String {
    let mut output = String::with_capacity(ascii_body_len(grid));
    for row in grid.rows() {
        write_ascii_row(row, &mut output);
    }
    output
}

/// Streaming counterpart to [`render_ascii`]: writes glyphs row by row to `out`.
///
/// Peak extra memory is one row's worth of scratch space, not the whole rendering,
/// so a 10,000 × 5,000-cell grid can be written to disk without ever materializing
/// the (potentially hundreds of megabytes) output string.
pub fn render_ascii_to<W: Write + ?Sized>(grid: &Grid, out: &mut W) -> Result<(), ImageError> {
    let mut scratch = String::with_capacity(grid.width() as usize + 1);
    for row in grid.rows() {
        scratch.clear();
        write_ascii_row(row, &mut scratch);
        out.write_all(scratch.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::Rgb;

    fn sample_grid() -> Grid {
        let cells = vec![
            Cell::new('a', Rgb::BLACK),
            Cell::new('b', Rgb::new(255, 0, 0)),
            Cell::new('c', Rgb::new(0, 255, 0)),
            Cell::new('d', Rgb::BLACK),
        ];
        Grid::from_cells(2, 2, cells).unwrap()
    }

    #[test]
    fn renders_glyphs_and_newlines_only() {
        let grid = sample_grid();
        let out = render_ascii(&grid);
        assert_eq!(out, "ab\ncd\n");
        assert!(!out.contains('\x1b'), "plain rendering must not emit escapes");
    }

    #[test]
    fn streaming_matches_batch() {
        let grid = sample_grid();
        let mut streamed = Vec::new();
        render_ascii_to(&grid, &mut streamed).unwrap();
        assert_eq!(String::from_utf8(streamed).unwrap(), render_ascii(&grid));
    }

    #[test]
    fn capacity_estimate_handles_multibyte_glyphs() {
        let cells = vec![
            Cell::new('░', Rgb::BLACK),
            Cell::new('█', Rgb::WHITE),
        ];
        let grid = Grid::from_cells(2, 1, cells).unwrap();
        let out = render_ascii(&grid);
        assert_eq!(out, "░█\n");
        assert_eq!(out.len(), ascii_body_len(&grid));
    }
}
