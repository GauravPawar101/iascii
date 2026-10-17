use crate::error::ImageError;
use crate::ramp::{Ramp, RampType, validate_ramp};

/// Formula used to compute grayscale luminance from RGB pixel values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LuminanceMethod {
    /// Arithmetic average: (R + G + B) / 3
    Average,
    /// Perceptual ITU-R BT.601 standard (0.299*R + 0.587*G + 0.114*B)
    #[default]
    Bt601,
    /// Perceptual ITU-R BT.709 standard (0.2126*R + 0.7152*G + 0.0722*B)
    Bt709,
}

impl LuminanceMethod {
    /// Calculates normalized luminosity [0.0, 1.0] from 8-bit RGB components.
    #[inline]
    pub fn calculate(&self, r: u8, g: u8, b: u8) -> f32 {
        let r = r as f32 / 255.0;
        let g = g as f32 / 255.0;
        let b = b as f32 / 255.0;

        match self {
            Self::Average => (r + g + b) / 3.0,
            Self::Bt601 => 0.299 * r + 0.587 * g + 0.114 * b,
            Self::Bt709 => 0.2126 * r + 0.7152 * g + 0.0722 * b,
        }
    }
}

/// Rendering quality tier trading speed for fidelity.
///
/// Quality affects only *how* brightness is computed and mapped onto the ramp,
/// never the output dimensions — combine it freely with [`OutputSizing`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    /// Fastest path: integer-only luminance arithmetic and direct ramp lookup.
    /// Best for previews, very large images, or hot loops.
    Good,
    /// Perceptual floating-point luminance (BT.601/BT.709 as configured) with
    /// direct ramp lookup. The default balance of speed and fidelity.
    #[default]
    Better,
    /// Perceptual luminance plus horizontal Floyd–Steinberg-style error
    /// diffusion across each row, producing visibly smoother gradients on
    /// low-arity ramps. Slightly slower than [`Quality::Better`].
    ///
    /// Error diffusion is confined to a single row, so sequential and
    /// `parallel` conversions still produce byte-identical output.
    Best,
}

/// Target output dimensions for ASCII conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputSizing {
    /// Explicit output width and height in characters.
    Explicit { width: usize, height: usize },
    /// Fixed maximum width with height calculated automatically using aspect ratio.
    MaxWidth(usize),
}

/// Aggregates user-facing settings controlling conversion and parallelism.
#[derive(Debug)]
pub struct Config {
    pub ramp: RampType,
    pub luminance_method: LuminanceMethod,
    pub quality: Quality,
    pub aspect_ratio_correction: f32,
    pub output_sizing: OutputSizing,
    pub parallel_threshold: usize,
}

impl Config {
    /// Instantiates a new builder with default settings.
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::default()
    }
}

/// Builder type for constructing `Config` with chainable setters and validation.
#[derive(Debug)]
pub struct ConfigBuilder {
    ramp: Option<RampType>,
    luminance_method: LuminanceMethod,
    quality: Quality,
    aspect_ratio_correction: f32,
    output_sizing: OutputSizing,
    parallel_threshold: usize,
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self {
            ramp: Some(RampType::Dark(Ramp::ascii())),
            luminance_method: LuminanceMethod::default(),
            quality: Quality::default(),
            aspect_ratio_correction: 0.5, // Standard terminal characters (~2:1 height:width ratio)
            output_sizing: OutputSizing::MaxWidth(80),
            parallel_threshold: 100, // Minimum rows required to activate thread pool
        }
    }
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ramp(mut self, ramp: RampType) -> Self {
        self.ramp = Some(ramp);
        self
    }

    pub fn luminance_method(mut self, method: LuminanceMethod) -> Self {
        self.luminance_method = method;
        self
    }

    /// Sets the speed/fidelity tier used during conversion (default [`Quality::Better`]).
    pub fn quality(mut self, quality: Quality) -> Self {
        self.quality = quality;
        self
    }

    pub fn aspect_ratio_correction(mut self, factor: f32) -> Self {
        self.aspect_ratio_correction = factor;
        self
    }

    pub fn output_sizing(mut self, sizing: OutputSizing) -> Self {
        self.output_sizing = sizing;
        self
    }

    pub fn max_width(mut self, max_width: usize) -> Self {
        self.output_sizing = OutputSizing::MaxWidth(max_width);
        self
    }

    pub fn explicit_dimensions(mut self, width: usize, height: usize) -> Self {
        self.output_sizing = OutputSizing::Explicit { width, height };
        self
    }

    pub fn parallel_threshold(mut self, min_rows: usize) -> Self {
        self.parallel_threshold = min_rows;
        self
    }

    /// Centralized validation producing `Result<Config, ImageError>`.
    pub fn build(self) -> Result<Config, ImageError> {
        // 1. Validate Ramp presence and non-emptiness
        let ramp_ref = self.ramp.as_ref().map(|r| r.inner());
        validate_ramp(ramp_ref)?;

        let ramp = self.ramp.ok_or(ImageError::EmptyRamp)?;

        // 2. Validate aspect ratio correction factor sanity
        if !self.aspect_ratio_correction.is_finite() || self.aspect_ratio_correction <= 0.0 {
            return Err(ImageError::InvalidAspectRatio(format!(
                "Aspect ratio correction factor must be a positive finite number, got {}",
                self.aspect_ratio_correction
            )));
        }

        // 3. Validate dimension parameters
        match self.output_sizing {
            OutputSizing::Explicit { width, height } => {
                if width == 0 || height == 0 {
                    return Err(ImageError::ZeroSizedImage(width, height));
                }
            }
            OutputSizing::MaxWidth(width) => {
                if width == 0 {
                    return Err(ImageError::ZeroSizedImage(0, 0));
                }
            }
        }

        Ok(Config {
            ramp,
            luminance_method: self.luminance_method,
            quality: self.quality,
            aspect_ratio_correction: self.aspect_ratio_correction,
            output_sizing: self.output_sizing,
            parallel_threshold: self.parallel_threshold,
        })
    }
}
