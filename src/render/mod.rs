pub mod ansi;

pub use ansi::render_ansi;

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
