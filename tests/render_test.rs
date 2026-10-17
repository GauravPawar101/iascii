//! Coverage for format selection (ASCII vs ANSI), quality tiers, sensible glyph
//! presets, and batch/streaming render parity.

use iascii::{
    ColorDepth, ConfigBuilder, ImageError, LuminanceMethod, Quality, Ramp, RampType, RenderFormat,
    convert_image, render, render_ansi, render_ansi_to, render_ascii, render_ascii_to, render_to,
};

/// Builds a flat RGB8 buffer filled with a single gray value.
fn uniform(width: u32, height: u32, value: u8) -> Vec<u8> {
    vec![value; (width * height * 3) as usize]
}

/// Builds a flat RGB8 buffer with a repeating byte pattern.
fn patterned(width: u32, height: u32) -> Vec<u8> {
    (0..(width * height * 3) as usize)
        .map(|i| (i * 7 % 256) as u8)
        .collect()
}

// --- Glyph presets ---

#[test]
fn sensible_ramp_presets_are_density_ordered() {
    // Clean printable ASCII, no punctuation ambiguity.
    assert_eq!(Ramp::ascii().as_str(), " .:-=+*#%@");
    assert!(Ramp::ascii().as_str().is_ascii());

    // Contiguous block shading.
    assert_eq!(Ramp::blocks().as_str(), " ░▒▓█");

    // The default ramp is the clean ASCII one.
    let config = ConfigBuilder::new().build().unwrap();
    assert_eq!(config.ramp.inner().as_str(), Ramp::ascii().as_str());
}

// --- Format selection ---

#[test]
fn ascii_output_is_escape_free_and_matches_display() {
    let config = ConfigBuilder::new()
        .ramp(RampType::Dark(Ramp::ascii()))
        .explicit_dimensions(8, 4)
        .build()
        .unwrap();
    let grid = convert_image(8, 4, &uniform(8, 4, 200), &config).unwrap();

    let ascii = render(&grid, RenderFormat::Ascii);

    assert_eq!(ascii, render_ascii(&grid));
    assert!(!ascii.contains('\x1b'), "plain ASCII must not contain escapes");
    assert_eq!(ascii.lines().count(), 4);
    assert!(ascii.ends_with('\n'));
    // `Display` is the escape-free rendering too.
    assert_eq!(format!("{grid}"), ascii);
}

#[test]
fn ansi_format_matches_legacy_renderer_for_every_depth() {
    let config = ConfigBuilder::new()
        .explicit_dimensions(6, 3)
        .build()
        .unwrap();
    let grid = convert_image(6, 3, &patterned(6, 3), &config).unwrap();

    for depth in [ColorDepth::TrueColor, ColorDepth::Ansi256, ColorDepth::Ansi16] {
        assert_eq!(
            render(&grid, RenderFormat::Ansi(depth)),
            render_ansi(&grid, depth),
            "dispatcher and direct ANSI rendering must agree for {depth:?}"
        );
    }
}

// --- Batch vs streaming ---

#[test]
fn streaming_writers_match_batch_renderers() {
    let config = ConfigBuilder::new()
        .explicit_dimensions(16, 8)
        .build()
        .unwrap();
    let grid = convert_image(16, 8, &patterned(16, 8), &config).unwrap();

    let mut out = Vec::new();
    render_ascii_to(&grid, &mut out).unwrap();
    assert_eq!(out, render_ascii(&grid).into_bytes());

    let mut out = Vec::new();
    render_ansi_to(&grid, ColorDepth::Ansi256, &mut out).unwrap();
    assert_eq!(out, render_ansi(&grid, ColorDepth::Ansi256).into_bytes());

    let mut out = Vec::new();
    render_to(&grid, RenderFormat::Ansi(ColorDepth::TrueColor), &mut out).unwrap();
    assert_eq!(
        out,
        render(&grid, RenderFormat::Ansi(ColorDepth::TrueColor)).into_bytes()
    );
}

/// A sink that only counts bytes and never stores them, proving the streaming
/// path can serve arbitrarily large output with O(1) sink memory.
struct CountingSink {
    bytes: usize,
}

impl std::io::Write for CountingSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.bytes += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn streams_large_output_without_materializing_it() {
    let dims = 512u32;
    let config = ConfigBuilder::new()
        .explicit_dimensions(dims as usize, dims as usize)
        .build()
        .unwrap();
    let grid = convert_image(dims, dims, &uniform(dims, dims, 40), &config).unwrap();

    let mut sink = CountingSink { bytes: 0 };
    render_ascii_to(&grid, &mut sink).unwrap();

    // dims glyphs per row + one newline, for dims rows.
    assert_eq!(sink.bytes, (dims as usize) * (dims as usize + 1));
}

#[test]
fn writer_errors_surface_as_image_error() {
    struct FailingSink;

    impl std::io::Write for FailingSink {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("disk full"))
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let config = ConfigBuilder::new()
        .explicit_dimensions(4, 2)
        .build()
        .unwrap();
    let grid = convert_image(4, 2, &uniform(4, 2, 10), &config).unwrap();

    let res = render_to(&grid, RenderFormat::Ascii, &mut FailingSink);
    assert!(matches!(res, Err(ImageError::Io(_))));
}

// --- Quality tiers ---

/// Builds a config for a single uniform-gray row at the given quality tier.
fn uniform_row_config(quality: Quality) -> iascii::Config {
    ConfigBuilder::new()
        .ramp(RampType::Dark(Ramp::ascii()))
        .luminance_method(LuminanceMethod::Average)
        .quality(quality)
        .explicit_dimensions(12, 1)
        .build()
        .unwrap()
}

#[test]
fn best_quality_diffuses_error_across_a_row() {
    // Mid-gray (128) lands on a fractional ramp position, so `Better` snaps every
    // cell to the same glyph while `Best` carries the remainder into the neighbour.
    let buf = uniform(12, 1, 128);
    let better = convert_image(12, 1, &buf, &uniform_row_config(Quality::Better)).unwrap();
    let best = convert_image(12, 1, &buf, &uniform_row_config(Quality::Best)).unwrap();

    let better_row: Vec<char> = better.row(0).unwrap().iter().map(|c| c.ch).collect();
    let best_row: Vec<char> = best.row(0).unwrap().iter().map(|c| c.ch).collect();

    assert!(
        better_row.iter().all(|&c| c == better_row[0]),
        "Better must not diffuse error: {better_row:?}"
    );
    assert!(
        best_row.iter().any(|&c| c != best_row[0]),
        "Best should dither a fractional value into a mix of glyphs: {best_row:?}"
    );
    // Dithering changes glyphs only, never dimensions.
    assert_eq!(better_row.len(), best_row.len());
}

#[test]
fn quality_tiers_agree_at_ramp_endpoints() {
    // Black and white have no rounding latitude, so all three tiers must match.
    let buf = uniform(8, 1, 255);
    let outputs: Vec<Vec<char>> = [Quality::Good, Quality::Better, Quality::Best]
        .into_iter()
        .map(|q| {
            let grid = convert_image(8, 1, &buf, &uniform_row_config(q)).unwrap();
            grid.row(0).unwrap().iter().map(|c| c.ch).collect()
        })
        .collect();

    assert!(outputs.iter().all(|row| *row == outputs[0]));
}

#[test]
fn good_quality_is_deterministic() {
    let buf = patterned(32, 8);
    let config = ConfigBuilder::new()
        .quality(Quality::Good)
        .explicit_dimensions(24, 6)
        .build()
        .unwrap();

    let a = convert_image(32, 8, &buf, &config).unwrap();
    let b = convert_image(32, 8, &buf, &config).unwrap();
    assert_eq!(a, b);
}

#[cfg(feature = "parallel")]
#[test]
fn best_quality_parallel_parity() {
    // Error diffusion is row-local on purpose, so parallel rows stay byte-identical.
    let buf = patterned(64, 48);
    let config = ConfigBuilder::new()
        .quality(Quality::Best)
        .explicit_dimensions(48, 40)
        .parallel_threshold(1)
        .build()
        .unwrap();

    let seq = convert_image(64, 48, &buf, &config).unwrap();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();
    let par = iascii::convert_with_pool(64, 48, &buf, &config, Some(&pool)).unwrap();

    assert_eq!(seq, par);
}

#[cfg(feature = "parallel")]
#[test]
fn parallel_parity_across_cell_chunk_boundaries() {
    // 30x40 = 1200 cells: split into 512-cell work items that cut through the
    // middle of rows, exercising the flat cell-splitting path for Good/Better.
    let buf = patterned(60, 80);
    for quality in [Quality::Good, Quality::Better, Quality::Best] {
        let config = ConfigBuilder::new()
            .quality(quality)
            .explicit_dimensions(30, 40)
            .parallel_threshold(1)
            .build()
            .unwrap();

        let seq = convert_image(60, 80, &buf, &config).unwrap();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(3)
            .build()
            .unwrap();
        let par = iascii::convert_with_pool(60, 80, &buf, &config, Some(&pool)).unwrap();

        assert_eq!(
            seq, par,
            "quality {quality:?} must stay identical across chunk boundaries"
        );
    }
}
