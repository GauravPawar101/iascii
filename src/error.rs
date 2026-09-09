use thiserror::Error;

#[derive(Error, Debug)]
pub enum ImageError {
    #[error("Both arguments should be non-zero, got width: {0}, height: {1}")]
    ZeroSizedImage(usize, usize),

    #[error("Buffer size mismatch: expected {0}, got {1}")]
    BufferSizeMismatch(usize, usize),

    #[error("Character ramp must not be empty")]
    EmptyRamp,

    #[error("Invalid aspect ratio: {0}")]
    InvalidAspectRatio(String),

    #[error("Invalid ramp: {0}")]
    InvalidRamp(String),

    #[error("Invalid configuration error")]
    InvalidConfiguration(String),
}
