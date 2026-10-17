use crate::error::ImageError;

/// A 24-bit RGB color representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0 };
    pub const WHITE: Self = Self {
        r: 255,
        g: 255,
        b: 255,
    };

    #[inline]
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

/// Represents a single output cell of ASCII/ANSI art containing a character and its color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub color: Rgb,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            color: Rgb::BLACK,
        }
    }
}

impl Cell {
    #[inline]
    pub const fn new(ch: char, color: Rgb) -> Self {
        Self { ch, color }
    }
}

/// The top-level output grid of character cells indexed in row-major order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grid {
    width: u32,
    height: u32,
    cells: Vec<Cell>,
}

impl Grid {
    // --- Restricted Constructors ---

    /// Allocates an empty `Grid` initialized with default cells.
    /// Restricted to `pub(crate)` so only `convert.rs` can construct instances.
    pub(crate) fn new(width: u32, height: u32) -> Self {
        let len = (width as usize) * (height as usize);
        Self {
            width,
            height,
            cells: vec![Cell::default(); len],
        }
    }

    /// Constructs a `Grid` from an existing flat vector of cells after verifying size consistency.
    #[allow(dead_code)] // exercised by unit tests and doctests; not used by the conversion pipelines
    pub(crate) fn from_cells(
        width: u32,
        height: u32,
        cells: Vec<Cell>,
    ) -> Result<Self, ImageError> {
        let expected_len = (width as usize) * (height as usize);
        if cells.len() != expected_len {
            return Err(ImageError::BufferSizeMismatch(expected_len, cells.len()));
        }

        Ok(Self {
            width,
            height,
            cells,
        })
    }

    // --- Public Immutable Accessors ---

    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Fetches a reference to a cell at the specified column (`x`) and row (`y`).
    #[inline]
    pub fn get(&self, x: u32, y: u32) -> Option<&Cell> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = (y * self.width + x) as usize;
        self.cells.get(idx)
    }

    /// Fetches an entire row as a slice.
    #[inline]
    pub fn row(&self, y: u32) -> Option<&[Cell]> {
        if y >= self.height {
            return None;
        }
        let start = (y * self.width) as usize;
        let end = start + (self.width as usize);
        Some(&self.cells[start..end])
    }

    /// Returns an iterator over immutable row slices.
    #[inline]
    pub fn rows(&self) -> std::slice::Chunks<'_, Cell> {
        // Grid dimensions are validated to be non-zero before a Grid is ever constructed
        // (see Config/convert validation), so `.max(1)` here is just a defensive guard
        // against chunks(0) panicking rather than a real code path.
        self.cells.chunks((self.width as usize).max(1))
    }

    /// Returns the underlying flat slice of all cells in row-major order.
    #[inline]
    pub fn as_slice(&self) -> &[Cell] {
        &self.cells
    }

    // --- Restricted Mutable Accessors ---

    /// Fetches a mutable reference to a cell at (`x`, `y`).
    ///
    /// Not currently called within the crate (`convert.rs`/`parallel.rs` use `row_mut` and
    /// `as_mut_slice` instead) — kept for API completeness / future callers.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn get_mut(&mut self, x: u32, y: u32) -> Option<&mut Cell> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = (y * self.width + x) as usize;
        self.cells.get_mut(idx)
    }

    /// Fetches a mutable slice of an entire row.
    #[inline]
    pub(crate) fn row_mut(&mut self, y: u32) -> Option<&mut [Cell]> {
        if y >= self.height {
            return None;
        }
        let start = (y * self.width) as usize;
        let end = start + (self.width as usize);
        Some(&mut self.cells[start..end])
    }

    /// Returns an iterator over mutable row slices (useful for sequential or parallel filling).
    ///
    /// Not currently called within the crate (`parallel.rs` chunks `as_mut_slice()` directly
    /// via rayon's `par_chunks_mut`) — kept for API completeness / future callers.
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn rows_mut(&mut self) -> std::slice::ChunksMut<'_, Cell> {
        self.cells.chunks_mut((self.width as usize).max(1))
    }

    /// Returns the underlying flat mutable slice of all cells.
    ///
    /// Only consumed by the `parallel` pipeline, which chunks it across worker threads.
    #[cfg_attr(not(feature = "parallel"), allow(dead_code))]
    #[inline]
    pub(crate) fn as_mut_slice(&mut self) -> &mut [Cell] {
        &mut self.cells
    }
}

// Place at the bottom of src/grid.rs

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ImageError;

    #[test]
    fn test_grid_indexing_and_accessors() {
        let mut cells = vec![Cell::default(); 6];
        cells[0] = Cell::new('A', Rgb::new(255, 0, 0));
        cells[5] = Cell::new('Z', Rgb::new(0, 255, 0));

        let grid = Grid::from_cells(3, 2, cells).unwrap();

        assert_eq!(grid.width(), 3);
        assert_eq!(grid.height(), 2);
        assert_eq!(grid.get(0, 0).unwrap().ch, 'A');
        assert_eq!(grid.get(2, 1).unwrap().ch, 'Z');
        assert!(grid.get(3, 0).is_none());

        let row1 = grid.row(1).unwrap();
        assert_eq!(row1.len(), 3);
        assert_eq!(row1[2].ch, 'Z');
    }

    #[test]
    fn test_grid_buffer_mismatch() {
        let cells = vec![Cell::default(); 5]; // Expected 3x2 = 6 cells
        let res = Grid::from_cells(3, 2, cells);
        assert!(matches!(res, Err(ImageError::BufferSizeMismatch(6, 5))));
    }
}
