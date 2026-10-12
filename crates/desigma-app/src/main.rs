mod app;
mod canvas;
mod cli;
mod clipboard;
mod comments;
mod comp_ops;
mod demo;
mod edit;
mod find;
mod fonts;
mod history;
mod home;
mod icons;
mod io;
mod layout;
mod libraries;
#[cfg(test)]
mod lint;
mod ops;
mod panels;
mod present;
mod proto;
mod richtext;
mod shell;
mod styles;
mod textedit;
mod textmeasure;
mod theme;
mod util;
mod variables;
mod vecfixture;
mod vecops;
mod widgets;

use std::sync::atomic::Ordering;

fn main() {
    let opts = match cli::parse(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("desigma: {e}\n{}", cli::USAGE);
            std::process::exit(2);
        }
    };
    if opts.help {
        println!("{}", cli::USAGE);
        return;
    }
    let (w, h) = opts.size.unwrap_or((theme::DEFAULT_WINDOW.0 as u32, theme::DEFAULT_WINDOW.1 as u32));
    let screenshot_mode = opts.screenshot.is_some();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title(theme::APP_TITLE)
        .with_inner_size([w as f32, h as f32])
        .with_min_inner_size([theme::MIN_WINDOW.0, theme::MIN_WINDOW.1]);
    if screenshot_mode {
        viewport = viewport.with_min_inner_size([200.0, 200.0]).with_resizable(true);
    }
    let native = eframe::NativeOptions { viewport, ..Default::default() };
    let result = eframe::run_native(theme::APP_TITLE, native, Box::new(move |cc| Ok(Box::new(shell::Shell::new(cc, opts)))));
    if let Err(e) = result {
        eprintln!("desigma: {e}");
        std::process::exit(1);
    }
    if screenshot_mode && !shell::SHOT_OK.load(Ordering::SeqCst) {
        eprintln!("desigma: screenshot was not captured");
        std::process::exit(1);
    }
}
