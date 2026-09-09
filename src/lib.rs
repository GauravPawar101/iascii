//! A high-performance, zero-allocation ASCII and ANSI terminal art converter.
//!
//! Provides image-to-text rendering, customizable character ramps, perceptual
//! luminance conversion models, and optional Rayon-powered parallel processing.

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
pub use config::{Config, ConfigBuilder, LuminanceMethod, OutputSizing};
pub use convert::convert_image;
pub use error::ImageError;
pub use grid::{Cell, Grid, Rgb};

#[cfg(feature = "parallel")]
pub use parallel::convert_with_pool;

pub use ramp::{Ramp, RampType};
pub use render::{ColorDepth, render_ansi};

/// Convenience type alias for operations producing [`ImageError`].
pub type Result<T> = std::result::Result<T, ImageError>;
