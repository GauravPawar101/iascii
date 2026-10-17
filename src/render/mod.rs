pub mod ansi;
pub mod ascii;

pub use ansi::{render_ansi, render_ansi_to};
pub use ascii::{render_ascii, render_ascii_to};

use crate::error::ImageError;
use crate::grid::Grid;
use std::fmt;
use std::io::Write;

/// Target terminal color capability when rendering a `Grid`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorDepth {
    /// 24-bit RGB true color (`\x1b[38;2;R;G;Bm`).
    #[default]
    TrueColor,
    /// 8-bit / 256-color palette (`\x1b[38;5;Nm`).
    Ansi256,
    /// Standard 4-bit / 16-color ANSI palette (`\x1b[30m`–`\x1b[37m`, `\x1b[90m`–`\x1b[97m`).
    Ansi16,
}

/// Selects how a [`Grid`] becomes text: plain glyphs, or ANSI-colored output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderFormat {
    /// Plain glyphs only, no escape sequences. Safe for logs, files, pipes, and any
    /// context that would otherwise print raw escape codes. This is the default.
    #[default]
    Ascii,
    /// ANSI-colored terminal output at the given [`ColorDepth`].
    Ansi(ColorDepth),
}

/// Batch render: builds and returns the entire rendering as a `String`.
///
/// Convenient when the output is small enough to hold in memory. For large
/// renderings, use [`render_to`], which streams and never materializes the whole
/// string.
///
/// ```
/// # use iascii::{Config, RenderFormat, ColorDepth, convert_image, render};
/// # let config = Config::builder().max_width(4).build().unwrap();
/// # let buf = vec![128u8; 4 * 4 * 3];
/// let grid = convert_image(4, 4, &buf, &config).unwrap();
/// let plain = render(&grid, RenderFormat::Ascii);
/// let colored = render(&grid, RenderFormat::Ansi(ColorDepth::Ansi256));
/// ```
pub fn render(grid: &Grid, format: RenderFormat) -> String {
    match format {
        RenderFormat::Ascii => ascii::render_ascii(grid),
        RenderFormat::Ansi(depth) => ansi::render_ansi(grid, depth),
    }
}

/// Streaming render: writes the rendering to any [`std::io::Write`] sink row by
/// row, so peak extra memory stays at about one row no matter how large the grid
/// or the resulting file is.
///
/// ```no_run
/// # use iascii::{Config, RenderFormat, ColorDepth, convert_image, render_to};
/// # fn run() -> iascii::Result<()> {
/// # let config = Config::builder().max_width(80).build().unwrap();
/// # let buf = vec![0u8; 64 * 64 * 3];
/// let grid = convert_image(64, 64, &buf, &config)?;
///
/// // Stream straight to a file, a socket, or stdout — no giant String in memory.
/// let mut file = std::fs::File::create("art.ansi")?;
/// render_to(&grid, RenderFormat::Ansi(ColorDepth::TrueColor), &mut file)?;
/// # Ok(())
/// # }
/// ```
pub fn render_to<W: Write + ?Sized>(
    grid: &Grid,
    format: RenderFormat,
    out: &mut W,
) -> Result<(), ImageError> {
    match format {
        RenderFormat::Ascii => ascii::render_ascii_to(grid, out),
        RenderFormat::Ansi(depth) => ansi::render_ansi_to(grid, depth, out),
    }
}

/// `Display` yields the plain (escape-free) rendering, so `println!("{grid}")`
/// just works. Use [`render`] when you need ANSI color or a specific format.
impl fmt::Display for Grid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use fmt::Write as _;
        for row in self.rows() {
            for cell in row {
                f.write_char(cell.ch)?;
            }
            f.write_char('\n')?;
        }
        Ok(())
    }
}
