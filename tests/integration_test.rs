use iascii::{
    ColorDepth, ConfigBuilder, ImageError, LuminanceMethod, Ramp, RampType, Rgb, convert_image,
    render_ansi,
};

#[test]
fn test_end_to_end_conversion_and_rendering() {
    // 1. Construct a synthetic 4x2 RGB input buffer
    // Row 0: Pure Black, Pure White, Pure Red, Pure Green
    // Row 1: Pure Blue, Mid Gray, Bright Yellow, Pure Black
    let width = 4u32;
    let height = 2u32;
    let img_buf: Vec<u8> = vec![
        0, 0, 0, 255, 255, 255, 255, 0, 0, 0, 255, 0, // Row 0
        0, 0, 255, 128, 128, 128, 255, 255, 0, 0, 0, 0, // Row 1
    ];

    // 2. Build configuration with explicit sizing and custom character ramp
    let custom_ramp = Ramp::new_custom(" .#").unwrap();
    let config = ConfigBuilder::new()
        .ramp(RampType::Dark(custom_ramp))
        .luminance_method(LuminanceMethod::Average)
        .explicit_dimensions(4, 2)
        .build()
        .unwrap();

    // 3. Perform image conversion to ASCII grid
    let grid = convert_image(width, height, &img_buf, &config).unwrap();

    assert_eq!(grid.width(), 4);
    assert_eq!(grid.height(), 2);

    // Verify individual sample points
    // Top-Left (Black) -> darkest character ' '
    assert_eq!(grid.get(0, 0).unwrap().ch, ' ');
    assert_eq!(grid.get(0, 0).unwrap().color, Rgb::BLACK);

    // Top-Right-ish (White) -> brightest character '#'
    assert_eq!(grid.get(1, 0).unwrap().ch, '#');
    assert_eq!(grid.get(1, 0).unwrap().color, Rgb::WHITE);

    // 4. Render output to ANSI string (TrueColor mode)
    let ansi_out = render_ansi(&grid, ColorDepth::TrueColor);

    // Verify ANSI color code emission and line resets
    assert!(ansi_out.contains("\x1b[38;2;255;255;255m")); // White escape sequence
    assert!(ansi_out.contains("\x1b[0m\n")); // End of line terminal reset
}

#[test]
fn test_invalid_input_buffer_handling() {
    let width = 10u32;
    let height = 10u32;
    let invalid_buf = vec![0u8; 100]; // Expected 10 * 10 * 3 = 300 bytes

    let config = ConfigBuilder::new().build().unwrap();
    let result = convert_image(width, height, &invalid_buf, &config);

    assert!(matches!(
        result,
        Err(ImageError::BufferSizeMismatch(300, 100))
    ));
}
