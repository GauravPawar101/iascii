use crate::error::ImageError;
use std::borrow::Cow;

// Default Static Ramp Sequences
pub const SMOOTH_GRADIENT: &str =
    " .'`^\",:;Il!i><~+_-?][}{1)(|\\/jtfrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$";
/// Clean, strictly density-ordered printable-ASCII ramp. Ten evenly spaced
/// glyphs with no punctuation ambiguity — the sensible default for plain-ASCII
/// output that has to survive copy/paste, logs, or fixed-width files.
pub const ASCII_RAMP: &str = " .:-=+*#%@";
/// Unicode block-shading ramp (`░▒▓█`). Renders as a smooth, contiguous
/// gradient in any terminal with basic box-drawing/block font coverage.
pub const BLOCKS_RAMP: &str = " ░▒▓█";
pub const QUANTIZED_RAMP: &str =
    "     .....:::::-----+++++*****=====%%%%%#####88888WWWWWMMMMM$$$$$@@@@@";
pub const INVERTED_SMOOTH: &str =
    "$@B%8&WM#*oahkbdpqwmZO0QLCJUYXzcvunxrftj\\/|()1{}[]?-_+~<>i!lI;:,\"^`'. ";
pub const HIGH_VARIETY: &str =
    " .'^\",:;~i|_ilI!><-+?]/\\|()1{}[]rcvunxzjftLCJUYXZO0Qoahkbdpqwm*WMB8&%$#N@";
pub const SEVEN_TIER: &str =
    "          ..........::::::::::----------**********##########@@@@@@@@@@";
pub const EMOJI_BRIGHTNESS: &str = "⬛⬛🌑🖤💣🌚🏴🕷️🦇♟️♠️♣️◼️◾▪️🔲🔳🟣🟦🟫🟥🟧🟩🟨⚙️🔒📌📍🎯👁️🔮🧿🌕🌝☀️⭐🌟✨💫⚡🔥💥🌼🌻💡🕯️❕❓❗◽▫️⚪🥛☁️❄️🧊🏳️💭💬🗯️🕊️📄📜✉️🏷️◽▫️⚪⬜⬜";
pub const CELESTIAL: &str = "⬛⬛⬛⬛⬛⬛⬛🌑🌑🌑🌑🌑🌑🌑🌒🌒🌒🌒🌒🌒🌒🌓🌓🌓🌓🌓🌓🌓🌔🌔🌔🌔🌔🌔🌔🌕🌕🌕🌕🌕🌕🌕🌝☀️⭐🌟✨💫💡🕯️⚪⚪⚪⚪⚪⚪⚪☁️☁️☁️❄️❄️🕊️🕊️📄📄⬜⬜⬜⬜";

/// Note on multi-codepoint glyphs: ramp indexing (see [`crate::sampling::fill_row`])
/// operates on `char` (Unicode scalar values), not grapheme clusters. Ramps built from
/// emoji sequences that combine multiple codepoints (base + variation selector, ZWJ
/// sequences, skin-tone modifiers, etc.) will have those sequences split across several
/// ramp "slots" rather than treated as one glyph. Fixing this fully requires grapheme
/// segmentation (e.g. the `unicode-segmentation` crate) and is a known limitation of the
/// current implementation rather than something silently miscomputed.
#[derive(Debug)]
pub enum Ramp {
    /// Clean density-ordered ASCII glyphs (` .:-=+*#%@`).
    Ascii(&'static str),
    /// Unicode block shading (` ░▒▓█`).
    Blocks(&'static str),
    SmoothGradient(&'static str),
    QuantizedRamp(&'static str),
    InvertedSmoothGradient(&'static str),
    HighVariety(&'static str),
    SevenTier(&'static str),
    EmojiBrightness(&'static str),
    Celestial(&'static str),
    CustomRamp(Cow<'static, str>),
}

impl Ramp {
    /// Clean, evenly spaced printable-ASCII ramp (` .:-=+*#%@`). The default.
    pub fn ascii() -> Self {
        Self::Ascii(ASCII_RAMP)
    }
    /// Unicode block-shading ramp (` ░▒▓█`).
    pub fn blocks() -> Self {
        Self::Blocks(BLOCKS_RAMP)
    }
    pub fn smooth_gradient() -> Self {
        Self::SmoothGradient(SMOOTH_GRADIENT)
    }
    pub fn quantized() -> Self {
        Self::QuantizedRamp(QUANTIZED_RAMP)
    }
    pub fn inverted_smooth() -> Self {
        Self::InvertedSmoothGradient(INVERTED_SMOOTH)
    }
    pub fn high_variety() -> Self {
        Self::HighVariety(HIGH_VARIETY)
    }
    pub fn seven_tier() -> Self {
        Self::SevenTier(SEVEN_TIER)
    }
    pub fn emoji_brightness() -> Self {
        Self::EmojiBrightness(EMOJI_BRIGHTNESS)
    }
    pub fn celestial() -> Self {
        Self::Celestial(CELESTIAL)
    }

    /// Creates a custom ramp and verifies both forward and inverted validity.
    pub fn new_custom(s: impl Into<Cow<'static, str>>) -> Result<Self, ImageError> {
        let cow = s.into();

        // 1. Validate forward string
        validate_str(&cow)?;

        // 2. Validate inverted string sequence (catches detached Unicode variation selectors)
        let inverted: String = cow.chars().rev().collect();
        validate_str(&inverted)?;

        Ok(Self::CustomRamp(cow))
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Ascii(s)
            | Self::Blocks(s)
            | Self::SmoothGradient(s)
            | Self::QuantizedRamp(s)
            | Self::InvertedSmoothGradient(s)
            | Self::HighVariety(s)
            | Self::SevenTier(s)
            | Self::EmojiBrightness(s)
            | Self::Celestial(s) => s,
            Self::CustomRamp(s) => s.as_ref(),
        }
    }

    /// Creates an inverted copy of the ramp sequence, validating the result the same way
    /// `new_custom` validates a forward/reversed pair (guards against producing an orphaned
    /// Unicode variation selector at the new leading position).
    pub fn invert(&self) -> Result<Self, ImageError> {
        let reversed: String = self.as_str().chars().rev().collect();
        validate_str(&reversed)?;
        Ok(Self::CustomRamp(Cow::Owned(reversed)))
    }
}

#[derive(Debug)]
pub enum RampType {
    Dark(Ramp),
    Light(Ramp),
}

impl RampType {
    #[inline]
    pub fn inner(&self) -> &Ramp {
        match self {
            Self::Dark(ramp) | Self::Light(ramp) => ramp,
        }
    }
}

/// Helper function to validate string payload integrity
fn validate_str(s: &str) -> Result<(), ImageError> {
    if s.is_empty() {
        return Err(ImageError::EmptyRamp);
    }

    // Reject unprintable control characters (\n, \r, \0)
    if s.bytes().any(|b| b == b'\n' || b == b'\r' || b == b'\0') {
        return Err(ImageError::InvalidRamp(
            "Ramp contains invalid control characters".into(),
        ));
    }

    // Ensure leading character is not a detached Unicode variation selector (\u{FE00}..=\u{FE0F})
    if let Some(first_char) = s.chars().next() {
        if ('\u{FE00}'..='\u{FE0F}').contains(&first_char) {
            return Err(ImageError::InvalidRamp(
                "Inverted ramp produces orphan variation selector".into(),
            ));
        }
    }

    Ok(())
}

#[inline]
pub fn validate_ramp(ramp: Option<&Ramp>) -> Result<(), ImageError> {
    let Some(ramp) = ramp else {
        return Err(ImageError::EmptyRamp);
    };

    validate_str(ramp.as_str())
}
