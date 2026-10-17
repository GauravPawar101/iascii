//! Demonstrates iascii's rendering options without needing an image decoder.
//!
//! A synthetic gradient image is converted once and then rendered several ways:
//!
//! ```text
//! cargo run --example render_demo            # plain ASCII (default)
//! cargo run --example render_demo -- ansi    # 24-bit ANSI color
//! cargo run --example render_demo -- large   # stream a huge rendering to a file
//! ```

use std::io::Write;

use iascii::{
    ColorDepth, ConfigBuilder, Quality, Ramp, RampType, RenderFormat, convert_image, render,
    render_to,
};

/// Builds a synthetic RGB8 image: red ramps across x, green down y, and blue is a
/// radial highlight — enough structure to show off ramps, color, and dithering.
fn synthetic_image(width: u32, height: u32) -> Vec<u8> {
    let mut buf = Vec::with_capacity((width * height * 3) as usize);
    let (cx, cy) = (width as f32 / 2.0, height as f32 / 2.0);
    let max_dist = (cx * cx + cy * cy).sqrt();

    for y in 0..height {
        for x in 0..width {
            let r = (x * 255 / width.max(1)) as u8;
            let g = (y * 255 / height.max(1)) as u8;
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let b = (255.0 * (1.0 - dist / max_dist).max(0.0)) as u8;
            buf.extend_from_slice(&[r, g, b]);
        }
    }
    buf
}

fn main() -> iascii::Result<()> {
    let (width, height) = (256u32, 256u32);
    let buf = synthetic_image(width, height);

    // Best quality uses error diffusion for smooth gradients; the clean ASCII ramp
    // keeps plain output readable anywhere.
    let config = ConfigBuilder::new()
        .ramp(RampType::Dark(Ramp::ascii()))
        .quality(Quality::Best)
        .max_width(80)
        .build()?;

    let grid = convert_image(width, height, &buf, &config)?;

    match std::env::args().nth(1).as_deref() {
        Some("ansi") => {
            // Batch: build the escaped string and hand it to the terminal.
            let colored = render(&grid, RenderFormat::Ansi(ColorDepth::TrueColor));
            println!("{colored}");
        }
        Some("large") => {
            // Streaming: render a very large grid straight to disk one row at a time.
            // Peak extra memory stays at about one row, not the whole file.
            let big = ConfigBuilder::new()
                .ramp(RampType::Dark(Ramp::ascii()))
                .quality(Quality::Good) // fastest tier for bulk output
                .explicit_dimensions(2000, 1000)
                .build()?;
            let big_grid = convert_image(width, height, &buf, &big)?;

            let path = "ascii-art.ansi";
            let mut file = std::fs::File::create(path)?;
            render_to(&big_grid, RenderFormat::Ascii, &mut file)?;
            file.flush()?;
            println!("streamed {} rows of ASCII to {path}", big_grid.height());
        }
        _ => {
            // Default: plain, escape-free ASCII.
            println!("{}", render(&grid, RenderFormat::Ascii));
        }
    }

    Ok(())
}
