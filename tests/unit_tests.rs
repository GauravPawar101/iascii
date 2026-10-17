use iascii::{ConfigBuilder, ImageError, LuminanceMethod, OutputSizing, Ramp, Rgb};

#[cfg(feature = "parallel")]
use iascii::{convert_image, convert_with_pool};

// --- Config Tests ---

#[test]
fn test_luminance_method_calculations() {
    let r = 255;
    let g = 0;
    let b = 0;

    // Red-only input under plain arithmetic average: 1/3 ~ 0.3333
    let avg = LuminanceMethod::Average.calculate(r, g, b);
    assert!((avg - 0.333333).abs() < 0.001);

    // ITU-R BT.601 red weighting: 0.299
    let bt601 = LuminanceMethod::Bt601.calculate(r, g, b);
    assert!((bt601 - 0.299).abs() < 0.001);

    // ITU-R BT.709 red weighting: 0.2126
    let bt709 = LuminanceMethod::Bt709.calculate(r, g, b);
    assert!((bt709 - 0.2126).abs() < 0.001);
}

#[test]
fn test_config_builder_defaults() {
    let config = ConfigBuilder::new().build().unwrap();

    assert_eq!(config.luminance_method, LuminanceMethod::Bt601);
    assert_eq!(config.aspect_ratio_correction, 0.5);
    assert_eq!(config.output_sizing, OutputSizing::MaxWidth(80));
    assert_eq!(config.parallel_threshold, 100);
}

#[test]
fn test_config_builder_validations() {
    // Reject non-positive aspect ratio factor
    let res = ConfigBuilder::new().aspect_ratio_correction(0.0).build();
    assert!(matches!(res, Err(ImageError::InvalidAspectRatio(_))));

    let res = ConfigBuilder::new().aspect_ratio_correction(-0.5).build();
    assert!(matches!(res, Err(ImageError::InvalidAspectRatio(_))));

    // Reject zero output dimensions
    let res = ConfigBuilder::new().max_width(0).build();
    assert!(matches!(res, Err(ImageError::ZeroSizedImage(0, 0))));

    let res = ConfigBuilder::new().explicit_dimensions(0, 50).build();
    assert!(matches!(res, Err(ImageError::ZeroSizedImage(0, 50))));
}

// --- Ramp Tests ---

#[test]
fn test_custom_ramp_validation() {
    // Valid custom ramp
    let ramp = Ramp::new_custom(" .:#@").unwrap();
    assert_eq!(ramp.as_str(), " .:#@");

    // Reject empty ramp
    let res = Ramp::new_custom("");
    assert!(matches!(res, Err(ImageError::EmptyRamp)));

    // Reject unprintable control characters
    let res = Ramp::new_custom("abc\ndef");
    assert!(matches!(res, Err(ImageError::InvalidRamp(_))));
}

#[test]
fn test_ramp_inversion() {
    let ramp = Ramp::smooth_gradient();
    let inverted = ramp.invert().unwrap();
    assert_eq!(
        inverted.as_str(),
        ramp.as_str().chars().rev().collect::<String>()
    );
}

// --- Sampling Tests ---

#[test]
fn test_pixel_and_block_sampling() {
    use iascii::sampling::{average_block, get_pixel_rgb, map_block_geometry};

    // 2x2 RGB image: Top-Left Red, Top-Right Green, Bottom-Left Blue, Bottom-Right White
    let img_buf = vec![
        255, 0, 0, 0, 255, 0, // Row 0
        0, 0, 255, 255, 255, 255, // Row 1
    ];

    // Out-of-bounds fallback check
    let oob = get_pixel_rgb(&img_buf, 2, 5, 5, 3);
    assert_eq!(oob, Rgb::BLACK);

    // Block geometry mapping (1x1 target grid mapping from 2x2 source)
    let (x_start, x_end, y_start, y_end) = map_block_geometry(0, 0, 1, 1, 2, 2);
    assert_eq!((x_start, x_end, y_start, y_end), (0, 2, 0, 2));

    // Block average of all 4 pixels
    let avg = average_block(&img_buf, 2, 3, x_start, x_end, y_start, y_end);
    assert_eq!(avg, Rgb::new(127, 127, 127)); // (255+0+0+255)/4 = 127 per channel
}

// --- Parallel Parity Tests ---

#[cfg(feature = "parallel")]
#[test]
fn test_parallel_sequential_parity() {
    let width = 8;
    let height = 8;
    // Create a deterministic checkerboard pattern byte buffer
    let mut buf = vec![0u8; (width * height * 3) as usize];
    for (i, byte) in buf.iter_mut().enumerate() {
        *byte = (i % 256) as u8;
    }

    let config = ConfigBuilder::new()
        .explicit_dimensions(4, 4)
        .parallel_threshold(1) // Force parallel activation
        .build()
        .unwrap();

    let seq_grid = convert_image(width, height, &buf, &config).unwrap();

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .unwrap();

    let par_grid = convert_with_pool(width, height, &buf, &config, Some(&pool)).unwrap();

    assert_eq!(seq_grid, par_grid);
}
