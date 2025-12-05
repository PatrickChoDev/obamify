//! Minimal headless generator to refresh `assignments.json` for a preset.
//! Usage: `cargo run --release --bin generate_assignments -- <preset-name> [sidelen] [out-dir]`
//! - `preset-name` refers to a folder under `presets/` that contains `source.png`.
//! - `sidelen` (optional) defaults to the source image width.
//! - `out-dir` (optional) defaults to `presets/<preset-name>`.

use obamify::app::calculate::util::{Algorithm, CropScale, GenerationSettings};
use obamify::app::calculate::{self, ProgressMsg};
use obamify::app::preset::{Preset, UnprocessedPreset};
use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::thread;

#[cfg(target_arch = "wasm32")]
fn main() {
    // Bins are not intended for wasm; provide a clear failure.
    panic!("generate_assignments is not available on wasm32");
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();

    let mut args = env::args().skip(1);
    // Flags: --algo <genetic|optimal>
    let mut algo = Algorithm::Optimal;
    let mut positional: Vec<String> = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--algo" => {
                if let Some(v) = args.next() {
                    algo = match v.as_str() {
                        "genetic" => Algorithm::Genetic,
                        "optimal" => Algorithm::Optimal,
                        other => panic!("unknown --algo {other}, expected genetic|optimal"),
                    };
                } else {
                    panic!("--algo requires a value (genetic|optimal)");
                }
            }
            _ => positional.push(arg),
        }
    }

    let preset_name = positional.get(0).cloned().expect(
        "usage: generate_assignments [--algo genetic|optimal] <preset-name> [sidelen] [out-dir]",
    );
    let sidelen_arg = positional.get(1).cloned();
    let out_dir_arg = positional.get(2).cloned();

    let source_path = PathBuf::from("presets")
        .join(&preset_name)
        .join("source.png");
    let out_dir = out_dir_arg
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("presets").join(&preset_name));

    let source = image::open(&source_path)?.to_rgb8();
    let sidelen = sidelen_arg
        .as_deref()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or_else(|| source.width());

    let mut settings =
        GenerationSettings::default(uuid::Uuid::new_v4(), format!("{preset_name}-regen"));
    settings.sidelen = sidelen;
    settings.algorithm = algo;
    settings.target_crop_scale = CropScale::identity();
    settings.source_crop_scale = CropScale::identity();

    let unprocessed = UnprocessedPreset {
        name: preset_name.clone(),
        width: source.width(),
        height: source.height(),
        source_img: source.into_raw(),
    };

    println!(
        "Generating assignments for `{preset_name}` at sidelen {sidelen} with {:?} -> {}",
        algo,
        out_dir.display()
    );

    let (tx, rx) = std::sync::mpsc::sync_channel(16);
    let cancel = Arc::new(AtomicBool::new(false));

    // Run on a worker thread so we can consume progress concurrently (avoids channel backpressure deadlocks).
    let cancel_clone = cancel.clone();
    let handle = thread::spawn(move || {
        let mut tx_clone = tx.clone();
        let res = match settings.algorithm {
            Algorithm::Optimal => {
                calculate::process_optimal(unprocessed, settings, &mut tx_clone, cancel_clone)
            }
            Algorithm::Genetic => {
                calculate::process_genetic(unprocessed, settings, &mut tx_clone, cancel_clone)
            }
        };
        if let Err(err) = res {
            let _ = tx_clone.send(ProgressMsg::Error(err.to_string()));
        }
    });

    // Consume progress logs until completion.
    let mut done = false;
    while let Ok(msg) = rx.recv() {
        match msg {
            ProgressMsg::Progress(p) => {
                eprintln!("progress: {:.1}%", p * 100.0);
            }
            ProgressMsg::UpdatePreview { .. } => {
                // ignore preview payloads; they just keep the UI responsive in-app
            }
            ProgressMsg::UpdateAssignments(_) => {
                // skip verbose assignment spam
            }
            ProgressMsg::Done(preset) => {
                write_results(&preset, &out_dir)?;
                println!("Saved assignments to {}", out_dir.display());
                done = true;
                break;
            }
            ProgressMsg::Error(err) => {
                eprintln!("Generation failed: {err}");
                break;
            }
            ProgressMsg::Cancelled => {
                eprintln!("Generation cancelled.");
                break;
            }
        }
    }

    // Ensure worker thread exits before finishing.
    let _ = handle.join();
    if !done {
        std::process::exit(1);
    }

    Ok(())
}

fn write_results(preset: &Preset, out_dir: &Path) -> Result<(), Box<dyn Error>> {
    std::fs::create_dir_all(out_dir)?;

    let sidelen = preset.inner.width;
    let source = &preset.inner.source_img;
    let assignments = &preset.assignments;

    // Rebuild output image by mapping source pixels through assignments.
    let mut output = vec![0u8; source.len()];
    for (dst_idx, src_idx) in assignments.iter().enumerate() {
        let dst_base = dst_idx * 3;
        let src_base = src_idx * 3;
        output[dst_base] = source[src_base];
        output[dst_base + 1] = source[src_base + 1];
        output[dst_base + 2] = source[src_base + 2];
    }

    let save_rgb = |name: &str, data: &[u8]| -> Result<(), Box<dyn Error>> {
        let buf =
            image::ImageBuffer::<image::Rgb<u8>, _>::from_vec(sidelen, sidelen, data.to_vec())
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("failed to build image buffer for {name}"),
                    )
                })?;
        buf.save(out_dir.join(name))?;
        Ok(())
    };

    save_rgb("source.png", source)?;
    if let Some(target) = &preset.target_img {
        save_rgb("target.png", target)?;
    }
    save_rgb("output.png", &output)?;

    std::fs::write(
        out_dir.join("assignments.json"),
        serde_json::to_string(assignments)?,
    )?;

    Ok(())
}
