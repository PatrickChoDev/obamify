//! Generate a grayscale weights mask from a target image.
//! Usage: `cargo run --bin generate_weights -- [input] [output] [size]`
//! - `input`  (optional): path to target image. Defaults to `src/app/calculate/target256.png`.
//! - `output` (optional): path to write weights. Defaults to `weights<side>.png` next to the input.
//! - `size`   (optional): square side length to resize to. Defaults to the input image width.
//!
//! The output is an 8-bit grayscale PNG where brighter = higher weight.

#[cfg(target_arch = "wasm32")]
fn main() {
    panic!("generate_weights is only for native builds");
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let mut args = std::env::args().skip(1);
    let input = args
        .next()
        .unwrap_or_else(|| "src/app/calculate/target256.png".to_string());
    let output_arg = args.next();
    let size_arg = args.next();

    let img = image::open(&input)?;
    let width = img.width();
    let height = img.height();
    if width != height {
        eprintln!(
            "warning: input is not square ({}x{}); weights will be based on its luminance",
            width, height
        );
    }

    let side = size_arg
        .as_deref()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(width.min(height));

    let mut gray = img.to_luma8();
    if gray.width() != side || gray.height() != side {
        gray = image::imageops::resize(&gray, side, side, image::imageops::FilterType::Lanczos3);
    }

    let output = output_arg.unwrap_or_else(|| {
        let stem = format!("weights{side}.png");
        std::path::Path::new(&input)
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(stem)
            .display()
            .to_string()
    });

    gray.save(&output)?;
    println!(
        "Wrote weights to {} ({}x{}, from {})",
        output,
        gray.width(),
        gray.height(),
        input
    );
    Ok(())
}
