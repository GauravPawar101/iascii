//! A fast ASCII and ANSI terminal art converter.
//!
//! Provides image-to-text rendering, customizable character ramps, perceptual
//! luminance conversion models, Good→Best quality tiers, and optional
//! Rayon-powered parallel processing.
//!
//! Rendering comes in two shapes: batch ([`render`], [`render_ascii`],
//! [`render_ansi`]) which return a `String`, and streaming ([`render_to`],
//! [`render_ascii_to`], [`render_ansi_to`]) which write to any
//! [`std::io::Write`] sink for arbitrarily large output.

pub mod config;
pub mod convert;
pub mod error;
pub mod grid;

#[cfg(feature = "parallel")]
pub mod parallel;

pub mod ramp;
pub mod render;
pub mod sampling;

// Re-exported crate public API
pub use config::{Config, ConfigBuilder, LuminanceMethod, OutputSizing, Quality};
pub use convert::convert_image;
pub use error::ImageError;
pub use grid::{Cell, Grid, Rgb};

#[cfg(feature = "parallel")]
pub use parallel::convert_with_pool;

pub use ramp::{Ramp, RampType};
pub use render::{
    ColorDepth, RenderFormat, render, render_ansi, render_ansi_to, render_ascii, render_ascii_to,
    render_to,
};

/// Convenience type alias for operations producing [`ImageError`].
pub type Result<T> = std::result::Result<T, ImageError>;
