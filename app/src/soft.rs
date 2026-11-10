//! Software rendering: the CPU draws the window (`raster.rs`) and softbuffer
//! copies it to the screen, so no OpenGL driver is loaded. On Windows that
//! driver is most of the GPU build's memory. Window and input come from winit
//! and egui-winit, the same pieces eframe uses.

use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use eframe::egui::{self, ViewportId, ViewportInfo};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Icon, Window, WindowId};

use crate::raster::Raster;

/// Last frame's drawing time in µs (memory line in Settings and the log).
static FRAME_US: AtomicU32 = AtomicU32::new(0);
/// Set once the software renderer is running.
static ACTIVE: AtomicU32 = AtomicU32::new(0);

pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed) != 0
}

pub fn last_frame_ms() -> f32 {
    FRAME_US.load(Ordering::Relaxed) as f32 / 1000.0
}

pub struct Options {
    pub title: &'static str,
    pub size: [f32; 2],
    pub min_size: [f32; 2],
    /// RGBA, width, height.
    pub icon: Option<(Vec<u8>, u32, u32)>,
}

/// Repaint request from egui (any thread): when, and the pass it was made in.
struct Wake {
    at: Instant,
    pass: u64,
}

struct Win {
    window: Rc<Window>,
    // Kept alive for the surface.
    _context: softbuffer::Context<Rc<Window>>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    state: egui_winit::State,
}

struct Shell {
    opts: Options,
    ctx: egui::Context,
    app: crate::ui::App,
    win: Option<Win>,
    raster: Raster,
    info: ViewportInfo,
    repaint_at: Option<Instant>,
    /// Set if the window could not be created (caller falls back to the GPU renderer).
    error: Option<String>,
}

/// Runs the UI until the window closes. Errs before any window was shown if
/// software rendering can't start, so the caller can use the GPU renderer.
pub fn run(opts: Options) -> Result<(), String> {
    let event_loop = EventLoop::<Wake>::with_user_event().build().map_err(|e| format!("event loop: {e}"))?;
    let proxy = Mutex::new(event_loop.create_proxy());
    let ctx = egui::Context::default();
    // Device threads and egui animations wake the loop through this.
    ctx.set_request_repaint_callback(move |info| {
        if let Ok(p) = proxy.lock() {
            let _ = p.send_event(Wake { at: Instant::now() + info.delay, pass: info.current_cumulative_pass_nr });
        }
    });
    let app = crate::ui::App::new(&ctx);
    let mut shell = Shell { opts, ctx, app, win: None, raster: Raster::default(), info: ViewportInfo::default(), repaint_at: None, error: None };
    event_loop.run_app(&mut shell).map_err(|e| format!("event loop: {e}"))?;
    match shell.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

impl Shell {
    fn create(&mut self, el: &ActiveEventLoop) -> Result<(), String> {
        let o = &self.opts;
        let mut attrs = Window::default_attributes()
            .with_title(o.title)
            .with_inner_size(LogicalSize::new(o.size[0], o.size[1]))
            .with_min_inner_size(LogicalSize::new(o.min_size[0], o.min_size[1]));
        if let Some((rgba, w, h)) = &o.icon {
            attrs = attrs.with_window_icon(Icon::from_rgba(rgba.clone(), *w, *h).ok());
        }
        let window = Rc::new(el.create_window(attrs).map_err(|e| format!("window: {e}"))?);
        let context = softbuffer::Context::new(window.clone()).map_err(|e| format!("softbuffer: {e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone()).map_err(|e| format!("softbuffer surface: {e}"))?;
        let state = egui_winit::State::new(self.ctx.clone(), ViewportId::ROOT, &*window, Some(window.scale_factor() as f32), window.theme(), Some(2048));
        egui_winit::update_viewport_info(&mut self.info, &self.ctx, &window, true);
        window.request_redraw();
        self.win = Some(Win { window, _context: context, surface, state });
        ACTIVE.store(1, Ordering::Relaxed);
        crate::log::line("renderer: software");
        Ok(())
    }

    fn schedule(&mut self, at: Instant) {
        self.repaint_at = Some(self.repaint_at.map_or(at, |t| t.min(at)));
    }

    fn paint(&mut self) {
        let Some(w) = self.win.as_mut() else { return };
        let started = Instant::now();
        let mut raw = w.state.take_egui_input(&w.window);
        egui_winit::update_viewport_info(&mut self.info, &self.ctx, &w.window, false);
        raw.viewports.insert(ViewportId::ROOT, self.info.clone());
        self.app.input_hook(&mut raw);
        let app = &mut self.app;
        let out = self.ctx.run(raw, |ctx| app.frame(ctx));
        w.state.handle_platform_output(&w.window, out.platform_output);

        self.raster.set_textures(&out.textures_delta);
        let size = w.window.inner_size();
        if let (Some(width), Some(height)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
            let prims = self.ctx.tessellate(out.shapes, out.pixels_per_point);
            match draw(&self.raster, &mut w.surface, &w.window, width, height, out.pixels_per_point, &prims) {
                Ok(()) => FRAME_US.store(started.elapsed().as_micros().min(u32::MAX as u128) as u32, Ordering::Relaxed),
                Err(e) => crate::log::line(format!("software renderer: {e}")),
            }
        }
        self.raster.free_textures(&out.textures_delta);

        if let Some(vo) = out.viewport_output.get(&ViewportId::ROOT) {
            let mut actions = egui::ahash::HashSet::default();
            egui_winit::process_viewport_commands(&self.ctx, &mut self.info, vo.commands.iter().cloned(), &w.window, &mut actions);
            if vo.repaint_delay.is_zero() {
                w.window.request_redraw();
            } else if vo.repaint_delay < Duration::from_secs(3600) {
                self.schedule(Instant::now() + vo.repaint_delay);
            }
        }
    }
}

fn draw(
    raster: &Raster,
    surface: &mut softbuffer::Surface<Rc<Window>, Rc<Window>>,
    window: &Window,
    width: NonZeroU32,
    height: NonZeroU32,
    ppp: f32,
    prims: &[egui::ClippedPrimitive],
) -> Result<(), String> {
    surface.resize(width, height).map_err(|e| e.to_string())?;
    let mut buf = surface.buffer_mut().map_err(|e| e.to_string())?;
    raster.paint(&mut buf, width.get() as usize, height.get() as usize, ppp, prims, crate::theme::pal().bg);
    window.pre_present_notify();
    buf.present().map_err(|e| e.to_string())
}

impl ApplicationHandler<Wake> for Shell {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.win.is_none() {
            if let Err(e) = self.create(el) {
                self.error = Some(e);
                el.exit();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::RedrawRequested => self.paint(),
            event => {
                let Some(w) = self.win.as_mut() else { return };
                if w.state.on_window_event(&w.window, &event).repaint {
                    w.window.request_redraw();
                }
            }
        }
    }

    fn user_event(&mut self, _el: &ActiveEventLoop, wake: Wake) {
        // A frame already ran since this request: it has been handled.
        if wake.pass == self.ctx.cumulative_pass_nr() {
            self.schedule(wake.at);
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        match self.repaint_at {
            Some(t) if t <= Instant::now() => {
                self.repaint_at = None;
                if let Some(w) = &self.win {
                    w.window.request_redraw();
                }
                el.set_control_flow(ControlFlow::Wait);
            }
            Some(t) => el.set_control_flow(ControlFlow::WaitUntil(t)),
            None => el.set_control_flow(ControlFlow::Wait),
        }
    }
}
