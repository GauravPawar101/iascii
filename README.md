# iascii

A small, fast Rust library for converting a decoded RGB image into ASCII or
colored ANSI terminal art.

You decode the image yourself (with the `image` crate, or anything else) and hand
this library the raw `width`, `height`, and RGB8 bytes. It maps blocks of pixels
down to a grid of characters, each carrying its own color, and renders that grid
either as **plain ASCII** or as **ANSI-escaped color** — as a `String` (batch) or
streamed straight to any writer (streaming).

---

## Features

- **Two render formats, one grid.** Convert once, then render as escape-free
  ASCII (`RenderFormat::Ascii`) or color at three ANSI depths
  (`RenderFormat::Ansi(ColorDepth::…)`) — chosen at render time, not conversion time.
- **Batch *and* streaming output.** `render` / `render_ascii` / `render_ansi`
  return a `String`; `render_to` / `render_ascii_to` / `render_ansi_to` write row
  by row to any `std::io::Write`. Streaming keeps peak extra memory at roughly
  **one row**, so you can generate output far larger than RAM.
- **Good → Best quality tiers.** `Quality::Good` uses a fixed-point luminance
  fast path, `Better` (default) uses perceptual floating-point luminance, and
  `Best` adds horizontal error-diffusion dithering for visibly smoother gradients.
- **Sensible built-in glyphs.** `Ramp::ascii()` (` .:-=+*#%@`, the default) is a
  clean, evenly spaced, ambiguity-free ASCII ramp; `Ramp::blocks()` (` ░▒▓█`) gives
  contiguous shading. Plus the classic presets and fully custom ramps.
- **Sync core, no forced runtime.** `convert_image` is a plain synchronous call —
  no tokio, no hidden threads. Call it from anywhere, including inside
  `spawn_blocking` if you're in an async context.
- **Optional parallelism, entirely caller-controlled.** Behind the `parallel`
  feature flag, `convert_with_pool` accepts an `Option<&rayon::ThreadPool>`. Pass
  your own pool, or `None` to fall back to the sequential path — the crate never
  touches rayon's global pool implicitly, so it won't oversubscribe threads when
  called concurrently from multiple places.
- **Structured output, not just a string.** Conversion produces a `Grid` of
  `Cell { ch, color }` values, so you can post-process it yourself (HTML, a TUI
  framework, a diff) without ever touching escape codes.
- **Configurable everything:** character ramp, luminance formula (plain average,
  BT.601, BT.709), light or dark ramp orientation, terminal aspect-ratio
  correction, output sizing, and (with `parallel`) the row-count threshold before
  concurrency kicks in.

## What this crate does *not* do

- It does not decode images. Bring your own decoder (e.g. the `image` crate) and
  pass in raw RGB8 bytes plus dimensions.
- It only accepts **RGB8** input (3 bytes per pixel, row-major, no alpha). Strip
  or blend alpha yourself before calling in.

---

## Installation

```toml
[dependencies]
iascii = "0.1"

# Optional: enables `convert_with_pool` and pulls in rayon.
# iascii = { version = "0.1", features = ["parallel"] }
```

## Quick start

```rust
use iascii::{ColorDepth, Config, RenderFormat, convert_image, render};

fn main() -> iascii::Result<()> {
    // However you got these — e.g. via the `image` crate:
    //   let img = image::open("photo.png")?.to_rgb8();
    //   let (width, height) = img.dimensions();
    //   let buf = img.into_raw();
    let width = 4;
    let height = 4;
    let buf: Vec<u8> = vec![128; (width * height * 3) as usize]; // flat RGB8 bytes

    let config = Config::builder().max_width(80).build()?;
    let grid = convert_image(width, height, &buf, &config)?;

    // Pick a format at render time.
    print!("{}", render(&grid, RenderFormat::Ascii));
    println!("{}", render(&grid, RenderFormat::Ansi(ColorDepth::TrueColor)));
    Ok(())
}
```

### Streaming large output

`render_to` writes row by row, so the full output never has to exist in memory —
useful for very large grids, log files, or piping to another process:

```rust
use iascii::{ColorDepth, Config, RenderFormat, convert_image, render_to};

fn main() -> iascii::Result<()> {
    let (width, height) = (1920, 1080);
    let buf: Vec<u8> = vec![0; width * height * 3];

    let config = Config::builder()
        .explicit_dimensions(1000, 600) // big character grid
        .build()?;
    let grid = convert_image(width, height, &buf, &config)?;

    // Straight to stdout, a file, a socket, a compressor — anything `Write`.
    let mut out = std::io::stdout().lock();
    render_to(&grid, RenderFormat::Ansi(ColorDepth::Ansi256), &mut out)?;
    Ok(())
}
```

## Parallel usage (feature `parallel`)

```rust
use iascii::{ColorDepth, Config, RenderFormat, convert_with_pool, render};

fn main() -> iascii::Result<()> {
    let width = 1920;
    let height = 1080;
    let buf: Vec<u8> = vec![0; (width * height * 3) as usize];

    let config = Config::builder()
        .max_width(160)
        .parallel_threshold(50) // only parallelize if the output has >= 50 cells
        .build()?;

    // Bring your own pool — the crate never touches rayon's global pool.
    let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();

    let grid = convert_with_pool(width, height, &buf, &config, Some(&pool))?;
    println!("{}", render(&grid, RenderFormat::Ansi(ColorDepth::Ansi256)));
    Ok(())
}
```

Passing `None` instead of `Some(&pool)` — or an output that doesn't clear
`parallel_threshold` cells — transparently falls back to the exact same
sequential code path used by `convert_image`, so output is always identical
regardless of which one runs.

Work is split by row for `Quality::Best` (its error diffusion is row-local) and by
cell otherwise, so wide-but-short grids parallelize instead of staying
single-threaded.

---

## Rendering

Conversion and rendering are separate steps, so one `Grid` can be rendered in as
many formats as you like.

### Formats

| Format | Entry point | Output |
|---|---|---|
| Plain ASCII | `RenderFormat::Ascii`, `render_ascii`, `render_ascii_to` | Glyphs + `\n` only, no escape sequences |
| ANSI color | `RenderFormat::Ansi(depth)`, `render_ansi`, `render_ansi_to` | Glyphs wrapped in color escapes |

ANSI color depths (chosen at render time, independent of conversion):

```rust
use iascii::ColorDepth;

ColorDepth::TrueColor; // 24-bit RGB:  \x1b[38;2;R;G;Bm
ColorDepth::Ansi256;   // 256-color:    \x1b[38;5;Nm
ColorDepth::Ansi16;    // 16-color:     \x1b[30m–\x1b[37m, \x1b[90m–\x1b[97m
```

`Grid` also implements `Display`, which yields the plain escape-free rendering, so
`println!("{grid}")` just works.

### Batch vs. streaming

| | Batch | Streaming |
|---|---|---|
| Functions | `render`, `render_ascii`, `render_ansi` | `render_to`, `render_ascii_to`, `render_ansi_to` |
| Returns | `String` | `Result<(), ImageError>` |
| Memory | Whole rendering in memory | ~one row |
| Use when | Output is small, or you need the full string anyway | Output is large, or goes straight to a file/socket/stdout |

Both paths produce identical bytes; the streaming functions simply avoid
materializing the result.

---

## Configuration reference

Built via `Config::builder()...build()`, which returns `Result<Config, ImageError>`
and validates everything in one place.

| Setter | Default | What it controls |
|---|---|---|
| `.ramp(RampType)` | `RampType::Dark(Ramp::ascii())` | Which characters represent which brightness, and whether bright pixels map to dense or sparse characters |
| `.luminance_method(LuminanceMethod)` | `Bt601` | How RGB is reduced to a single brightness value — `Average`, `Bt601`, or `Bt709` |
| `.quality(Quality)` | `Better` | Speed/fidelity tier — `Good`, `Better`, or `Best` (see below) |
| `.aspect_ratio_correction(f32)` | `0.5` | Compensates for terminal characters being taller than they are wide |
| `.max_width(usize)` / `.explicit_dimensions(w, h)` | `MaxWidth(80)` | Output sizing — either a max width with height derived automatically, or exact dimensions |
| `.parallel_threshold(usize)` | `4096` | (with `parallel`) Minimum output **cells** before a supplied thread pool is actually used |

### Quality tiers

| Tier | Brightness math | Dithering | When to use |
|---|---|---|---|
| `Quality::Good` | Fixed-point integers (no `f32`) | No | Previews, huge images, hot loops |
| `Quality::Better` *(default)* | Perceptual `f32` (BT.601/BT.709) | No | Balanced daily use |
| `Quality::Best` | Perceptual `f32` | Horizontal error diffusion | Smoothest gradients, final output |

`Best` diffuses quantization error left-to-right within each row, turning a
fractional brightness into a mix of the two neighbouring glyphs instead of
snapping to one. Diffusion is deliberately **row-local**, so rows stay
independent and sequential vs. `parallel` output remains byte-identical.

Quality affects only brightness mapping — never output dimensions.

### Ramps

`RampType::Dark(ramp)` maps bright pixels to visually dense characters (the right
choice for the typical dark-background terminal). `RampType::Light(ramp)` inverts
that, for light-background output.

Built-in `Ramp` presets:

| Preset | Glyphs | Notes |
|---|---|---|
| `ascii()` *(default)* | ` .:-=+*#%@` | Clean, evenly spaced, printable ASCII — copy/paste and log safe |
| `blocks()` | ` ░▒▓█` | Contiguous Unicode block shading |
| `smooth_gradient()` | 70 glyphs | Classic fine-grained density ramp |
| `quantized()`, `seven_tier()` | repeated blocks | Coarse, posterized looks |
| `inverted_smooth()`, `high_variety()` | mixed | Classic alternatives |
| `emoji_brightness()`, `celestial()` | emoji | Best-effort (see the limitation below) |

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

---

## How it works

1. **Validate** — buffer length must equal `width * height * 3`; dimensions must
   be non-zero.
2. **Compute target size** — source dimensions + your sizing config + aspect
   correction → a concrete `(grid_width, grid_height)` in character cells.
3. **Sample, row by row** — for each output cell, average the block of source
   pixels it represents, reduce that to a luminance value (integer for `Good`,
   perceptual for `Better`/`Best`), apply dithering under `Best`, and look up the
   matching ramp character. The column → source-x geometry is computed once per
   conversion and shared across all rows and threads; row geometry is computed
   once per row. Sequential by default; concurrent with `parallel`.
4. **Return a `Grid`** — structured `Cell { ch, color }` data. Valid to consume
   directly (e.g. in a TUI) without ever rendering to text.
5. **Render (optional, in any format)** — batch to a `String`, or stream row by
   row to a writer. The ANSI path tracks the active color so redundant escape
   codes are skipped.

Because output resolution is a small character grid rather than the source's
full pixel count, expect a blocky, color-approximate impression of the image —
that's inherent to ASCII/ANSI art, not a bug.

---

## Error handling

All fallible entry points return `iascii::Result<T>` (`Result<T, ImageError>`).
Notable variants: `ZeroSizedImage`, `BufferSizeMismatch`, `EmptyRamp`,
`InvalidRamp`, `InvalidAspectRatio`, and `Io` (propagated from streaming writers).

---

## License

Licensed under either of MIT or Apache-2.0, at your option.
