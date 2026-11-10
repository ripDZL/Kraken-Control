#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
// Tray-agent plumbing is Windows-only; other hosts build the UI for demo/screenshots.
#![cfg_attr(not(any(windows, feature = "check-win")), allow(dead_code))]

mod agent;
mod backup;
mod device;
mod device2;
mod hid;
mod icon;
mod log;
mod mem;
mod raster;
mod store;
mod soft;
mod sys;
mod theme;
mod ui;
mod ui2;

/// Counts Kraken's own heap for the memory line in the log and Settings.
#[global_allocator]
static ALLOC: mem::Counting = mem::Counting;

fn main() -> eframe::Result {
    // Rename cleanup (Deer Control -> Kraken Control) before anything touches the settings folder.
    // Portable copies skip it: nothing of theirs lives in %APPDATA% or the registry.
    let portable = store::portable_dir();
    let notes: Vec<String> = match &portable {
        Some(dir) => vec![format!("portable mode: settings and log in {}", dir.display())],
        None => sys::migrate_legacy().into_iter().chain(store::migrate_legacy_dir()).collect(),
    };
    let args: Vec<String> = std::env::args().collect();
    let agent = args.iter().any(|a| a == "--agent");
    log::line(format!("start kraken-control {}{}", env!("CARGO_PKG_VERSION"), if agent { " (tray agent)" } else { "" }));
    for n in notes {
        log::line(n);
    }
    if agent {
        // Tray mode: no window, no GL.
        std::process::exit(agent::run());
    }
    let demo = args.iter().any(|a| a == "--demo" || a == "--demo-gen2");
    // "Restart now" (Settings): the old window is still closing, wait for it.
    let restarted = args.iter().any(|a| a == "--restarted");
    let mut owns_instance = demo || sys::claim_instance("Local\\KrakenControlUi");
    for _ in 0..50 {
        if owns_instance || !restarted {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        owns_instance = sys::claim_instance("Local\\KrakenControlUi");
    }
    if !owns_instance {
        // Already open: focus that window instead of opening a second one.
        if let Some(w) = sys::find_ui_window() {
            sys::bring_to_front(w);
        }
        return Ok(());
    }
    const SIZE: [f32; 2] = [1180.0, 760.0];
    const MIN_SIZE: [f32; 2] = [960.0, 600.0];
    let software = if args.iter().any(|a| a == "--gpu") {
        false
    } else {
        args.iter().any(|a| a == "--software") || store::Store::load().settings.renderer == store::Renderer::Software
    };
    if software {
        let opts = soft::Options { title: sys::UI_TITLE, size: SIZE, min_size: MIN_SIZE, icon: icon::rgba(64).map(|rgba| (rgba, 64, 64)) };
        match soft::run(opts) {
            Ok(()) => return Ok(()),
            // No window could be made. winit allows one event loop per process,
            // so start again with the GPU renderer.
            Err(e) => {
                log::line(format!("software renderer unavailable ({e}); restarting with GPU renderer"));
                if sys::spawn_self(&["--gpu", "--restarted"]).is_ok() {
                    return Ok(());
                }
            }
        }
    }
    let mut viewport = eframe::egui::ViewportBuilder::default().with_title(sys::UI_TITLE).with_inner_size(SIZE).with_min_inner_size(MIN_SIZE);
    if let Some(rgba) = icon::rgba(64) {
        viewport = viewport.with_icon(std::sync::Arc::new(eframe::egui::IconData { rgba, width: 64, height: 64 }));
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    log::line("renderer: GPU (OpenGL)");
    eframe::run_native(sys::UI_TITLE, options, Box::new(|cc| Ok(Box::new(ui::App::new(&cc.egui_ctx)))))
}
