//! Turn a coloured picture into a monochrome template image for the menu bar:
//! light pixels become opaque, dark ones (outlines, shadows) transparent, so
//! macOS can tint the result white or black like its own status icons.
//!
//!     cargo run --release --manifest-path scripts/menubar-template/Cargo.toml -- IN.png OUT.png SIZE [light|dark]
//!
//! "light" (the default, what `src-tauri/icons/tray.png` is made with) keeps
//! the light pixels, "dark" keeps the dark ones.

use image::imageops::{resize, FilterType};
use image::{ImageBuffer, Luma, Rgba, Rgba32FImage, RgbaImage};
use std::process::exit;

// The alpha ramp runs between these two luminances: mid-tones stay
// semi-transparent, which keeps the shading of the picture instead of
// flattening it into one shape.
const LO: f32 = 0.28;
const HI: f32 = 0.55;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (input, output, size) = match args.as_slice() {
        [_, input, output, size] | [_, input, output, size, _] => (input, output, size),
        _ => usage(),
    };
    let keep_light = match args.get(4).map(String::as_str) {
        None | Some("light") => true,
        Some("dark") => false,
        Some(_) => usage(),
    };
    let Some(size) = size.parse::<u32>().ok().filter(|s| *s > 0) else {
        usage()
    };
    let src = match image::open(input) {
        Ok(img) => img.to_rgba32f(),
        Err(e) => {
            eprintln!("{input}: {e}");
            exit(1);
        }
    };

    // Work at 4x the target size, then downsample for smooth edges. The
    // picture is premultiplied for the first resize so that transparent
    // pixels do not bleed their colour into the edges.
    let work = size * 4;
    let mut pre = Rgba32FImage::new(src.width(), src.height());
    for (p, q) in src.pixels().zip(pre.pixels_mut()) {
        let a = p[3];
        *q = Rgba([p[0] * a, p[1] * a, p[2] * a, a]);
    }
    let big = resize(&pre, work, work, FilterType::Lanczos3);

    // alpha = source alpha * lightness (or darkness).
    let mut alpha: ImageBuffer<Luma<f32>, Vec<f32>> = ImageBuffer::new(work, work);
    for (p, q) in big.pixels().zip(alpha.pixels_mut()) {
        let a = p[3].clamp(0.0, 1.0);
        if a <= 0.0 {
            continue;
        }
        let (r, g, b) = (p[0] / a, p[1] / a, p[2] / a);
        let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let dark = ((HI - lum) / (HI - LO)).clamp(0.0, 1.0);
        *q = Luma([a * if keep_light { 1.0 - dark } else { dark }]);
    }
    let small = resize(&alpha, size, size, FilterType::Triangle);

    let mut out = RgbaImage::new(size, size);
    for (p, q) in small.pixels().zip(out.pixels_mut()) {
        let a = (p[0].clamp(0.0, 1.0) * 255.0).round() as u8;
        *q = Rgba([0, 0, 0, a]); // black, the system tints it
    }
    if let Err(e) = out.save(output) {
        eprintln!("{output}: {e}");
        exit(1);
    }
}

fn usage() -> ! {
    eprintln!("usage: menubar-template IN.png OUT.png SIZE [light|dark]");
    exit(2);
}
