# iascii

A small Rust library for converting a decoded RGB image into colored ASCII/ANSI
terminal art.

You decode the image yourself (with the `image` crate, or anything else) and hand
this library the raw `width`, `height`, and RGB8 bytes. It maps blocks of pixels
down to a grid of characters, each carrying its own color, and can render that grid
into an ANSI-escaped string ready to `print!()`.

> Rename this crate in `Cargo.toml` to whatever you're actually calling it —
> `iascii` is just a placeholder used throughout this README.

---

## Features

- **Sync core, no forced runtime.** The primary `convert_image` function is a
  plain synchronous call — no tokio, no hidden threads. Call it from anywhere,
  including inside `spawn_blocking` if you're in an async context.
- **Optional parallelism, entirely caller-controlled.** Behind the `parallel`
  feature flag, `convert_with_pool` accepts an `Option<&rayon::ThreadPool>`. Pass
  your own pool, or `None` to fall back to the sequential path — the crate never
  touches rayon's global pool implicitly, so it won't oversubscribe threads when
  called concurrently from multiple places.
- **Structured output, not just a string.** Conversion produces a `Grid` of
  `Cell { ch, color }` values. Rendering to an ANSI string is a separate step
  (`render_ansi`), so you can post-process the grid yourself (HTML, a TUI
  framework, a diff) without ever touching escape codes.
- **Configurable everything:** character ramp (several built-in presets or a
  custom charset), luminance formula (plain average, BT.601, BT.709), light or
  dark ramp orientation, terminal aspect-ratio correction, output sizing, and
  (with `parallel`) the row-count threshold before concurrency kicks in.
- **Three ANSI color depths** at render time — truecolor, 256-color, and
  16-color — chosen independently of how the image was converted.

## What this crate does *not* do

- It does not decode images. Bring your own decoder (e.g. the `image` crate) and
  pass in raw RGB8 bytes plus dimensions.
- It only accepts **RGB8** input (3 bytes per pixel, row-major, no alpha). Strip
  or blend alpha yourself before calling in.

---

## Installation

```toml
[dependencies]
iascii = { path = "../img2ansi" }        # or a version once published

# Optional: enables `convert_with_pool` and pulls in rayon.
iascii = { path = "../img2ansi", features = ["parallel"] }
```

## Quick start

```rust
use iascii::{Config, ColorDepth, convert_image, render_ansi};

fn main() -> iascii::Result<()> {
    // However you got these — e.g. via the `image` crate:
    //   let img = image::open("photo.png")?.to_rgb8();
    //   let (width, height) = img.dimensions();
    //   let buf = img.into_raw();
    let width = 4;
    let height = 4;
    let buf: Vec<u8> = vec![128; (width * height * 3) as usize]; // flat RGB8 bytes

    let config = Config::builder()
        .max_width(80)
        .build()?;

    let grid = convert_image(width, height, &buf, &config)?;
    let ansi = render_ansi(&grid, ColorDepth::TrueColor);

    print!("{ansi}");
    Ok(())
}
```

## Parallel usage (feature `parallel`)

```rust
use iascii::{Config, ColorDepth, convert_with_pool, render_ansi};

fn main() -> iascii::Result<()> {
    let width = 1920;
    let height = 1080;
    let buf: Vec<u8> = vec![0; (width * height * 3) as usize];

    let config = Config::builder()
        .max_width(160)
        .parallel_threshold(50) // only parallelize if the output has >= 50 rows
        .build()?;

    // Bring your own pool — the crate never touches rayon's global pool.
    let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();

    let grid = convert_with_pool(width, height, &buf, &config, Some(&pool))?;
    println!("{}", render_ansi(&grid, ColorDepth::Ansi256));
    Ok(())
}
```

Passing `None` instead of `Some(&pool)` — or an image whose row count doesn't
clear `parallel_threshold` — transparently falls back to the exact same
sequential code path used by `convert_image`, so output is always identical
regardless of which one runs.

---

## Configuration reference

Built via `Config::builder()...build()`, which returns `Result<Config, ImageError>`
and validates everything in one place.

| Setter | Default | What it controls |
|---|---|---|
| `.ramp(RampType)` | `RampType::Dark(Ramp::smooth_gradient())` | Which characters represent which brightness, and whether bright pixels map to dense or sparse characters |
| `.luminance_method(LuminanceMethod)` | `Bt601` | How RGB is reduced to a single brightness value — `Average`, `Bt601`, or `Bt709` |
| `.aspect_ratio_correction(f32)` | `0.5` | Compensates for terminal characters being taller than they are wide |
| `.max_width(usize)` / `.explicit_dimensions(w, h)` | `MaxWidth(80)` | Output sizing — either a max width with height derived automatically, or exact dimensions |
| `.parallel_threshold(usize)` | `100` | (with `parallel`) Minimum output rows before a supplied thread pool is actually used |

### Ramps

`RampType::Dark(ramp)` maps bright pixels to visually dense characters (the
right choice for the typical dark-background terminal). `RampType::Light(ramp)`
inverts that, for light-background output.

Built-in `Ramp` presets: `smooth_gradient()`, `quantized()`, `inverted_smooth()`,
`high_variety()`, `seven_tier()`, `emoji_brightness()`, `celestial()`.

Custom ramps:

```rust
use iascii::{Ramp, RampType};

let ramp = RampType::Dark(Ramp::new_custom(" .:-=+*#%@")?);
```

> **Known limitation:** ramp indexing works on `char` (Unicode scalar values),
> not grapheme clusters. Multi-codepoint emoji (base character + variation
> selector, ZWJ sequences, etc.) can occupy more than one ramp slot and shift
> the brightness mapping. The built-in `emoji_brightness()` / `celestial()`
> ramps are best-effort for this reason — prefer single-codepoint custom ramps
> if exact alignment matters.

### Color depth

Chosen at render time, independent of conversion:

```rust
use iascii::ColorDepth;

render_ansi(&grid, ColorDepth::TrueColor); // 24-bit, \x1b[38;2;R;G;Bm
render_ansi(&grid, ColorDepth::Ansi256);   // 256-color palette
render_ansi(&grid, ColorDepth::Ansi16);    // basic 16-color, widest terminal support
```

---

## How it works

1. **Validate** — buffer length must equal `width * height * 3`; dimensions must
   be non-zero.
2. **Compute target size** — source dimensions + your sizing config + aspect
   correction → a concrete `(grid_width, grid_height)` in character cells.
3. **Sample, row by row** — for each output cell, average the block of source
   pixels it represents, reduce that to a luminance value, and look up the
   matching ramp character. Sequential by default; row-parallel with `parallel`.
4. **Return a `Grid`** — structured `Cell { ch, color }` data. Valid to consume
   directly (e.g. in a TUI) without ever rendering to ANSI.
5. **Render (optional)** — `render_ansi` walks the grid, tracks the active
   color to avoid redundant escape codes, and emits a printable `String`.

Because output resolution is a small character grid rather than the source's
full pixel count, expect a blocky, color-approximate impression of the image —
that's inherent to ASCII/ANSI art, not a bug.

---

## Error handling

All fallible entry points return `iascii::Result<T>` (`Result<T, ImageError>`).
Notable variants: `ZeroSizedImage`, `BufferSizeMismatch`, `EmptyRamp`,
`InvalidRamp`, `InvalidAspectRatio`.

---

## License

Add your license of choice here (MIT / Apache-2.0 are common for Rust crates).
