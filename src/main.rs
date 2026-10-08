// SPDX-License-Identifier: Apache-2.0
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod app;
mod viewport;
use opp_viewer::package::Package;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "OPP Viewer 0.1.0\nUsage: opp-viewer [FILE.opp] [--inspect] [--demo]\nOpen or drop .opp packages in the native window. --inspect prints JSON without opening a window."
        );
        return Ok(());
    }
    let demo = args.iter().any(|a| a == "--demo");
    let inspect = args.iter().any(|a| a == "--inspect");
    let file = args.iter().find(|a| !a.starts_with('-')).map(PathBuf::from);
    let initial = if let Some(path) = file {
        Some(Package::open(&path)?)
    } else if demo {
        Some(Package::from_bytes(
            include_bytes!("../fixtures/block-as-built.opp"),
            "block-as-built.opp",
        )?)
    } else {
        None
    };
    if inspect {
        let package = initial.context("--inspect requires FILE.opp or --demo")?;
        println!("{}", serde_json::to_string_pretty(&package.report())?);
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1380., 900.])
            .with_min_inner_size([900., 620.]),
        ..Default::default()
    };
    eframe::run_native(
        "OPP Viewer",
        options,
        Box::new(move |cc| Ok(Box::new(app::Viewer::new(cc, initial)))),
    )
    .map_err(|e| anyhow::anyhow!("Could not start the native window: {e}"))
}
use anyhow::Context;
