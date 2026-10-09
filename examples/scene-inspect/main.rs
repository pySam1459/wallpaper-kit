//! Read-only scene inspection through the library's public API.

use scene_renderer::{SceneSource, inspect_scene};
use serde_json::json;
use std::{
    error::Error,
    fs,
    path::PathBuf,
    process::ExitCode,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("scene-inspect: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let mut package = None;
    let mut assets = None;
    let mut output = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--package") => {
                package = Some(PathBuf::from(args.next().ok_or("--package needs a path")?))
            }
            Some("--assets") => {
                assets = Some(PathBuf::from(
                    args.next().ok_or("--assets needs a directory")?,
                ))
            }
            Some("--output") => {
                output = Some(PathBuf::from(
                    args.next().ok_or("--output needs a directory")?,
                ))
            }
            Some("--help") => {
                println!(
                    "scene-inspect --package <scene.pkg> --assets <engine assets> [--output <empty evidence directory>]"
                );
                return Ok(ExitCode::SUCCESS);
            }
            _ => return Err(format!("unknown argument: {}", arg.to_string_lossy()).into()),
        }
    }
    let source = SceneSource {
        package: package.ok_or("--package is required")?,
        engine_assets: assets.ok_or("--assets is required")?,
    };
    let output = output.unwrap_or_else(|| {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock precedes epoch")
            .as_millis();
        PathBuf::from(format!(
            "artifacts/inspection-{}-{stamp}",
            std::process::id()
        ))
    });
    fs::create_dir_all(&output)?;
    if fs::read_dir(&output)?.next().is_some() {
        return Err("evidence directory must be empty".into());
    }
    let start = Instant::now();
    let report = match inspect_scene(&source) {
        Ok(report) => report,
        Err(error) => {
            fs::write(
                output.join("failure.json"),
                serde_json::to_vec_pretty(&json!({"status":"failed", "error":error.to_string()}))?,
            )?;
            return Err(error.into());
        }
    };
    let blocking = report.has_blocking_findings();
    let summary = json!({
        "status": if blocking { "findings" } else { "inspected" },
        "rendering_supported": false, "inspection_ms": start.elapsed().as_secs_f64() * 1000.,
        "objects": report.scene.objects.len(), "assets": report.assets.len(), "textures": report.textures.len(),
        "diagnostics": report.diagnostics.len(), "report": output.join("report.json"),
    });
    fs::write(
        output.join("report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    fs::write(
        output.join("summary.json"),
        serde_json::to_vec_pretty(&summary)?,
    )?;
    println!("{summary}");
    Ok(if blocking {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    })
}
