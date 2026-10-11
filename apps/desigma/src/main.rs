//! Desigma: a free, offline design tool.

mod app;
mod canvas;
mod icons;
mod layers;
mod properties;
mod theme;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let path = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Desigma")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([800.0, 500.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Desigma",
        options,
        Box::new(|cc| Ok(Box::new(app::DesigmaApp::new(cc, path)))),
    )
}
