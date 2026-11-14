//! egui front end.

use std::collections::BTreeSet;
use std::time::{Duration, Instant, SystemTime};

use dd_proto::catalog::{self, CATEGORIES};
use dd_proto::knob::{self, KnobEvent, KnobMap};
use dd_proto::model::KNOB_POS;
use dd_proto::packets::{self, CommonFlags, Travel};
use dd_proto::rtp::{self, MatchMode, RdtPair};
use dd_proto::{led, pattern};
use dd_proto::{Identity, KeyAction, KeyDef, Keymap, Layer, Model, SLOTS};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::backup;
use crate::device::{self, Cmd, Event, Snapshot, TravelSet, Values};
use crate::store::{self, AgentConfig, AppRule, LightPreset1, Profile, Store};
use crate::sys;
use crate::theme::{self, pal, Cap, Page, PresetAction, ThemeId, UiFont};

/// Which board family the window shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Board {
    /// A75 Master / Ultra gen-1 (VID 0x352D).
    Gen1,
    /// 2025 A75 Ultra and other VID 0x19F5 boards.
    Gen2,
}

/// Pages of the gen-1 UI (shared list, see `theme::Page`).
type Tab = Page;

/// Last Win and RDT are mutually exclusive (official UI rule).
#[derive(Clone, Copy, PartialEq, Eq)]
enum PairMode {
    Off,
    LastWin,
    Rdt,
}

const DEFAULT_AP: f32 = 2.0;

pub struct App {
    dev: device::Handle,
    gen2: crate::ui2::Gen2Panel,
    board: Board,
    store: Store,

    identity: Option<Identity>,
    model: Model,
    status: String,
    status_is_error: bool,
    busy: bool,

    // Working state; `*_synced` = last value read from the keyboard.
    travel: TravelSet,
    travel_synced: Option<TravelSet>,
    flags: CommonFlags,
    flags_synced: Option<CommonFlags>,
    lw: Vec<(u8, u8)>,
    lw_synced: Option<Vec<(u8, u8)>>,
    rdt: Vec<RdtPair>,
    rdt_synced: Option<Vec<RdtPair>>,
    match_mode: Option<MatchMode>,
    match_synced: Option<Option<MatchMode>>,
    lw_replace: bool,
    lw_replace_synced: Option<bool>,
    /// Onboard profile slot count (firmware >= 81).
    slots: Option<u8>,
    keymap: Keymap,
    keymap_dirty: bool,
    /// Gen-1 knob (app copy; written on Save, not readable).
    knob: KnobMap,
    knob_dirty: bool,
    /// Knob event being edited on the Keymap page (instead of a key).
    knob_event: Option<KnobEvent>,
    /// Cached per model (parsing the embedded JSON every frame is wasteful).
    factory: Keymap,
    layout: Vec<KeyDef>,

    // UI state.
    tab: Tab,
    selected: BTreeSet<u8>,
    layer: Layer,
    /// Pair being built on the Pairs tab: kind and first key.
    pick: Option<(PairMode, Option<u8>)>,
    new_profile_name: String,
    /// `--demo`: fake connected A75 Master, no device I/O.
    demo: bool,
    /// Window icon, also shown in the header.
    icon: Option<egui::TextureHandle>,

    // Apps tab (tray agent).
    agent_cfg: AgentConfig,
    agent_cfg_stamp: Option<SystemTime>,
    agent_running: bool,
    autostart: bool,
    agent_checked: Option<Instant>,
    running_apps: Vec<String>,
    new_rule_exe: String,
    new_rule_slot: u8,

    // Lighting tab (gen-1, write-only: `light` is what Kraken last sent).
    light: store::Lighting1,
    light_paint: [u8; 3],
    light_paint2: [u8; 3],
    light_dir: pattern::Direction,
    light_palette: usize,
    /// Brush stroke in progress: last pointer position.
    light_stroke: Option<Pos2>,
    light_undo: Vec<Vec<[u8; 3]>>,
    /// Pending lighting write and when it may go.
    light_due: Option<Instant>,
    /// Name typed for a new lighting preset.
    light_preset_name: String,
    /// Export / import: file dialog running on a worker thread (export?, chosen path).
    backup_job: Option<std::sync::mpsc::Receiver<(bool, Option<std::path::PathBuf>)>>,
    /// File read and checked, waiting for the user to choose what to import.
    backup_pending: Option<PendingImport>,
    /// Start time and last memory log line (see `log_memory`).
    started: Instant,
    mem_logged: Option<Instant>,
    /// Live key depth (Performance page): switched on, last frame in mm, and
    /// whether the device thread was told to track.
    live_depth: bool,
    depth: Option<Box<device::Values>>,
    tracking_sent: bool,
}

impl App {
    pub fn new(egui_ctx: &egui::Context) -> Self {
        let ctx = egui_ctx.clone();
        let dev = device::spawn(move || ctx.request_repaint());
        let demo_gen2 = std::env::args().any(|a| a == "--demo-gen2");
        let gen2 = crate::ui2::Gen2Panel::new(egui_ctx.clone(), demo_gen2);
        let store = Store::load();
        // `--theme=` / `--font=` (screenshots) are not saved.
        let theme_id = std::env::args().find_map(|a| a.strip_prefix("--theme=").and_then(ThemeId::from_arg)).unwrap_or(store.settings.theme);
        let font = std::env::args().find_map(|a| a.strip_prefix("--font=").and_then(UiFont::from_arg)).or(store.settings.font);
        theme::apply(egui_ctx, theme_id, font);
        crate::log::set_packet_logging(store.settings.log_packets);
        let model = Model::A75Master;
        let args: Vec<String> = std::env::args().collect();
        let demo = args.iter().any(|a| a == "--demo");
        let mut app = Self {
            dev,
            gen2,
            board: if demo_gen2 { Board::Gen2 } else { Board::Gen1 },
            keymap: store.keymap(model),
            knob: store.knob(model),
            knob_dirty: false,
            knob_event: None,
            light: store.lighting(model),
            store,
            identity: None,
            model,
            status: "Looking for keyboard…".into(),
            status_is_error: false,
            busy: false,
            travel: TravelSet { ap: [DEFAULT_AP; SLOTS], ds: [0.5; SLOTS], us: [0.5; SLOTS] },
            travel_synced: None,
            flags: CommonFlags::default(),
            flags_synced: None,
            lw: Vec::new(),
            lw_synced: None,
            rdt: Vec::new(),
            rdt_synced: None,
            match_mode: None,
            match_synced: None,
            lw_replace: false,
            lw_replace_synced: None,
            slots: None,
            keymap_dirty: false,
            factory: Keymap::factory(model),
            layout: model.layout(),
            tab: Tab::Performance,
            selected: BTreeSet::new(),
            layer: Layer::Default,
            pick: None,
            new_profile_name: String::new(),
            demo,
            icon: crate::icon::rgba(64).map(|rgba| {
                egui_ctx.load_texture("kraken", egui::ColorImage::from_rgba_unmultiplied([64, 64], &rgba), egui::TextureOptions::LINEAR)
            }),
            agent_cfg: AgentConfig::load(),
            agent_cfg_stamp: AgentConfig::stamp(),
            agent_running: false,
            autostart: false,
            agent_checked: None,
            running_apps: Vec::new(),
            new_rule_exe: String::new(),
            new_rule_slot: 0,
            light_paint: [255, 60, 30],
            light_paint2: [0, 60, 255],
            light_dir: pattern::Direction::LeftToRight,
            light_palette: 0,
            light_stroke: None,
            light_undo: Vec::new(),
            light_due: None,
            light_preset_name: String::new(),
            backup_job: None,
            backup_pending: None,
            started: Instant::now(),
            mem_logged: None,
            live_depth: false,
            depth: None,
            tracking_sent: false,
        };
        if let Some(page) = args.iter().find_map(|a| a.strip_prefix("--tab=")).and_then(Page::from_arg) {
            app.tab = page;
        }
        let rdt_demo = args.iter().any(|a| a == "--tab=rdt");
        // `--import=<file>` opens the import dialog for that file (screenshots / testing).
        let import = args.iter().find_map(|a| a.strip_prefix("--import=")).map(std::path::PathBuf::from);
        if demo {
            app.live_depth = args.iter().any(|a| a == "--live-depth");
            app.enter_demo();
            if rdt_demo {
                app.set_pair_mode(PairMode::Rdt);
                app.pick = None;
            }
        }
        if let Some(path) = import {
            match backup::read(&path) {
                Ok(file) => {
                    let (keyboard, kraken) = (file.keyboard.is_some(), file.kraken.is_some());
                    app.backup_pending = Some(PendingImport { path, file, keyboard, kraken });
                }
                Err(e) => app.set_status(e, true),
            }
        }
        app
    }

    fn enter_demo(&mut self) {
        // Identity reply captured from a real A75 Master (fw 85).
        const ID: [u8; 37] = [
            0xa0, 0x02, 0x00, 0x01, 0x0b, 0x05, 0x04, 0x55, 0x00, 0, 0, 0, 0, 0, 0x01, 0x01, 0x01, 0, 0, 0x01, 0, 0, 0x12,
            0x06, 0x09, 0, 0, 0, 0, 0x01, 0, 0x02, 0, 0x02, 0x02, 0x03, 0x04,
        ];
        let id = Identity::parse(&ID).expect("demo identity");
        let mut t = self.travel.clone();
        for (i, v) in t.ap.iter_mut().enumerate() {
            *v = [2.0, 1.2, 0.6, 0.4, 3.3][i % 5];
        }
        for s in [44, 64, 65, 66] {
            t.ap[s] = 0.2;
            t.ds[s] = 0.05;
            t.us[s] = 0.05;
        }
        let snap = Snapshot {
            identity: id,
            travel: t,
            lw: vec![(64, 66)],
            rdt: vec![RdtPair { main: 44, trigger: 65, active_mm: 0.1, reset_mm: 2.0 }],
            slots: Some(3),
        };
        self.apply_snapshot(snap);
        if self.live_depth {
            // The captured identity has Turbo on, which blocks live depth.
            self.flags.turbo = false;
        }
        match self.tab {
            Tab::Performance if self.live_depth => self.selected.clear(),
            Tab::Performance => self.selected = [44u8, 64, 65, 66].into_iter().collect(),
            Tab::Keymap => {
                self.selected = [63u8].into_iter().collect();
                self.keymap.set(Layer::Default, 63, KeyAction::hid(41));
                self.keymap_dirty = true;
            }
            Tab::Advanced => {
                self.flags.rapid_trigger = true;
                self.lw.push((44, 65));
                self.pick = Some((PairMode::LastWin, Some(43)));
            }
            Tab::Lighting => self.light.custom = true,
            Tab::Profiles | Tab::Settings => {
                // In memory only; not written to agent.json.
                self.agent_cfg = AgentConfig {
                    auto_switch: true,
                    default_slot: Some(0),
                    rules: vec![
                        AppRule { exe: "valorant.exe".into(), slot: 1 },
                        AppRule { exe: "cs2.exe".into(), slot: 2 },
                    ],
                };
            }
        }
        self.set_status("Demo mode — no keyboard I/O", false);
    }

    fn connected_model(&self) -> Option<Model> {
        self.identity.as_ref().and_then(|i| i.model)
    }

    fn key_name(&self, slot: u8) -> &'static str {
        self.layout.iter().find(|k| k.slot == slot).map(|k| k.label).unwrap_or("?")
    }

    // ---------------------------------------------------------- dirty state

    /// Travel tables that differ from the keyboard (at wire resolution).
    fn travel_dirty_kinds(&self) -> Vec<Travel> {
        let Some(s) = &self.travel_synced else { return Vec::new() };
        let diff = |a: &Values, b: &Values, k: Travel| {
            a.iter().zip(b).any(|(x, y)| packets::mm_to_units(k, *x) != packets::mm_to_units(k, *y))
        };
        [(Travel::Ap, &self.travel.ap, &s.ap), (Travel::Ds, &self.travel.ds, &s.ds), (Travel::Us, &self.travel.us, &s.us)]
            .into_iter()
            .filter(|(k, a, b)| diff(a, b, *k))
            .map(|(k, _, _)| k)
            .collect()
    }

    fn travel_dirty(&self) -> bool {
        !self.travel_dirty_kinds().is_empty()
    }

    fn flags_dirty(&self) -> bool {
        self.flags_synced.is_some_and(|f| f != self.flags)
    }

    /// Pair tables are written when their mode is on and the table changed
    /// or the mode is being switched on.
    fn pairs_dirty(&self) -> bool {
        let Some(f) = self.flags_synced else { return false };
        let lw = self.flags.last_win && (!f.last_win || self.lw_synced.as_ref() != Some(&self.lw));
        let rdt = self.flags.rdt && (!f.rdt || !self.rdt_synced.as_ref().is_some_and(|s| same_rdt(s, &self.rdt)));
        lw || rdt
    }

    fn match_dirty(&self) -> bool {
        self.match_synced.is_some_and(|m| m != self.match_mode)
    }

    fn lw_replace_dirty(&self) -> bool {
        self.lw_replace_synced.is_some_and(|v| v != self.lw_replace)
    }

    fn any_dirty(&self) -> bool {
        self.travel_dirty()
            || self.flags_dirty()
            || self.keymap_dirty
            || self.knob_dirty
            || self.pairs_dirty()
            || self.match_dirty()
            || self.lw_replace_dirty()
    }

    fn set_status(&mut self, s: impl Into<String>, err: bool) {
        self.status = s.into();
        self.status_is_error = err;
    }

    // ---------------------------------------------------------- device events

    fn poll_device(&mut self) {
        while let Ok(ev) = self.dev.rx.try_recv() {
            match ev {
                Event::Connected(id) => {
                    match id.model {
                        Some(m) => {
                            if m != self.model {
                                self.model = m;
                                self.factory = Keymap::factory(m);
                                self.layout = m.layout();
                                self.keymap = self.store.keymap(m);
                                self.keymap_dirty = false;
                                self.knob = self.store.knob(m);
                                self.knob_dirty = false;
                                self.knob_event = None;
                                self.light = self.store.lighting(m);
                                self.light_undo.clear();
                                self.selected.clear();
                            }
                            let mut msg = format!("Connected: {} (firmware {})", m.name(), id.firmware_label);
                            if id.upgrade_recommended() {
                                msg.push_str(" — official driver recommends a firmware update");
                            }
                            self.set_status(msg, false);
                        }
                        None => self.set_status(
                            format!(
                                "Unsupported keyboard (type bytes {:02x} {:02x} {:02x}). Only A75 Master/Ultra are supported.",
                                id.type_bytes[0], id.type_bytes[1], id.type_bytes[2]
                            ),
                            true,
                        ),
                    }
                    self.apply_identity(&id);
                    self.identity = Some(id);
                }
                Event::Snapshot(s) => self.apply_snapshot(*s),
                Event::Flags(id) => {
                    self.apply_identity(&id);
                    self.identity = Some(id);
                }
                Event::Disconnected(why) => {
                    self.identity = None;
                    self.travel_synced = None;
                    self.flags_synced = None;
                    self.lw_synced = None;
                    self.rdt_synced = None;
                    self.match_synced = None;
                    self.lw_replace_synced = None;
                    self.slots = None;
                    self.pick = None;
                    self.busy = false;
                    self.depth = None;
                    self.tracking_sent = false;
                    self.set_status(format!("Disconnected ({why}). Looking for keyboard…"), true);
                }
                Event::Travel(t) => {
                    self.travel = (*t).clone();
                    self.travel_synced = Some(*t);
                }
                Event::Pairs { lw, rdt } => {
                    self.lw = lw.clone();
                    self.lw_synced = Some(lw);
                    self.rdt = rdt.clone();
                    self.rdt_synced = Some(rdt);
                }
                Event::Slots(n) => self.slots = Some(n),
                Event::Busy(b) => self.busy = b,
                Event::Done(msg) => self.set_status(msg, false),
                Event::Error(msg) => self.set_status(msg, true),
                Event::Depth(d) => self.depth = Some(d),
                Event::TrackingStopped(msg) => {
                    self.live_depth = false;
                    self.depth = None;
                    self.tracking_sent = false;
                    self.set_status(msg, true);
                }
            }
        }
    }

    fn apply_snapshot(&mut self, s: Snapshot) {
        self.apply_identity(&s.identity);
        self.identity = Some(s.identity);
        self.travel = s.travel.clone();
        self.travel_synced = Some(s.travel);
        self.lw = s.lw.clone();
        self.lw_synced = Some(s.lw);
        self.rdt = s.rdt.clone();
        self.rdt_synced = Some(s.rdt);
        self.slots = s.slots;
        self.pick = None;
    }

    fn apply_identity(&mut self, id: &Identity) {
        let f = CommonFlags {
            turbo: id.turbo,
            rapid_trigger: id.rapid_trigger,
            last_win: id.last_win,
            rdt: id.rdt,
            rt_match: id.rt_match,
        };
        self.flags = f;
        self.flags_synced = Some(f);
        let m = MatchMode::from_wire(id.auto_match_mode);
        self.match_mode = m;
        self.match_synced = Some(m);
        self.lw_replace = id.lw_replace;
        self.lw_replace_synced = Some(id.lw_replace);
    }

    fn save_to_keyboard(&mut self) {
        // Same order as the official driver: switches + actuation, then pair tables.
        let pairs = self.pairs_dirty();
        if self.flags_dirty() {
            self.dev.send(Cmd::SaveCommon(self.flags));
        }
        let kinds = self.travel_dirty_kinds();
        if !kinds.is_empty() {
            self.dev.send(Cmd::SaveTravel { set: Box::new(self.travel.clone()), kinds });
        }
        if self.match_dirty() {
            self.dev.send(Cmd::SetMatchMode(self.match_mode));
        }
        if self.lw_replace_dirty() {
            self.dev.send(Cmd::SetLwReplace(self.lw_replace));
        }
        if pairs {
            self.dev.send(Cmd::SavePairs {
                last_win: self.flags.last_win,
                rdt: self.flags.rdt,
                lw: self.lw.clone(),
                rdt_pairs: self.rdt.clone(),
            });
        }
        if self.keymap_dirty {
            self.dev.send(Cmd::SaveKeymap(Box::new(self.keymap.clone())));
            self.store.set_keymap(self.model, &self.keymap);
            self.keymap_dirty = false;
            self.persist();
        }
        if self.knob_dirty && self.knob_supported() {
            self.dev.send(Cmd::SaveKnob(Box::new(self.knob)));
            self.store.set_knob(self.model, &self.knob);
            self.knob_dirty = false;
            self.persist();
        }
    }

    /// Official driver: knob for A75 Master / Ultra with firmware 0.063+.
    fn knob_supported(&self) -> bool {
        self.demo || self.identity.as_ref().is_some_and(|i| i.model.is_some() && i.firmware >= knob::MIN_FIRMWARE)
    }

    fn persist(&mut self) {
        if let Err(e) = self.store.save() {
            self.set_status(format!("Could not save app state: {e}"), true);
        }
    }

    // ---------------------------------------------------------- lighting

    /// Lighting tab (gen-1). The keyboard can't report its lighting, so this
    /// shows what Kraken last sent; every change goes out straight away.
    fn lighting_tab(&mut self, ui: &mut egui::Ui) {
        let before = self.light.clone();
        if !self.light.sent {
            theme::help(ui, "The keyboard can't report its current lighting; this shows Kraken's defaults until you pick something.");
            ui.add_space(4.0);
        }
        if self.flags.turbo {
            ui.colored_label(pal().unsaved, "Turbo mode is on: the keyboard keeps lighting static until Turbo is off.");
        }
        ui.label("Effect");
        ui.horizontal_wrapped(|ui| {
            if ui.add(egui::Button::new("Custom (per key)").selected(self.light.custom).min_size(Vec2::new(110.0, 26.0))).clicked() {
                self.light.custom = true;
            }
            for e in led::EFFECTS {
                let on = !self.light.custom && self.light.effect == e.code;
                if ui.add(egui::Button::new(e.name).selected(on).min_size(Vec2::new(110.0, 26.0))).clicked() {
                    self.light.custom = false;
                    self.light.effect = e.code;
                    if self.light.colour == led::COLOURFUL && !e.colourful {
                        self.light.colour = 1;
                    }
                }
            }
        });
        let fx = if self.light.custom { None } else { led::effect_by_code(self.light.effect) };
        let off = fx.is_some_and(|e| e.code == led::OFF);
        ui.add_space(6.0);
        if !off {
            ui.spacing_mut().slider_width = 260.0;
            ui.horizontal(|ui| {
                ui.add_sized([120.0, 18.0], egui::Label::new("Brightness"));
                ui.add(egui::Slider::new(&mut self.light.bright, 0..=led::MAX_LEVEL));
            });
            if !self.light.custom {
                ui.horizontal(|ui| {
                    ui.add_sized([120.0, 18.0], egui::Label::new("Speed"));
                    ui.add(egui::Slider::new(&mut self.light.speed, 0..=led::MAX_LEVEL));
                });
            }
        }
        if let Some(e) = fx.filter(|e| e.preset || e.colourful) {
            ui.horizontal_wrapped(|ui| {
                ui.add_sized([120.0, 18.0], egui::Label::new("Colour"));
                if e.colourful && ui.add(egui::Button::new("Colourful").selected(self.light.colour == led::COLOURFUL)).clicked() {
                    self.light.colour = led::COLOURFUL;
                }
                if e.preset {
                    for (i, (name, c)) in led::PRESETS.iter().enumerate() {
                        let idx = i as u8 + 1;
                        let (r, p) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                        ui.painter().rect_filled(r, 4.0, Color32::from_rgb(c[0], c[1], c[2]));
                        let stroke = if self.light.colour == idx { Stroke::new(2.0_f32, pal().text) } else { Stroke::new(1.0_f32, pal().disabled) };
                        ui.painter().rect_stroke(r, 4.0, stroke, StrokeKind::Inside);
                        if p.on_hover_text(*name).clicked() {
                            self.light.colour = idx;
                        }
                    }
                }
            });
        }
        if self.light.custom {
            ui.separator();
            self.light_custom_tools(ui);
        }
        ui.add_space(4.0);
        theme::help(ui, "Lighting is sent to the keyboard straight away.");
        if self.light != before {
            let held = ui.input(|i| i.pointer.primary_down());
            self.light_due = Some(Instant::now() + if held { Duration::from_millis(150) } else { Duration::ZERO });
        }
    }

    /// Lighting presets (saved on this PC): one click applies a look live.
    fn light_presets(&mut self, ui: &mut egui::Ui) {
        let model = self.model;
        let idx: Vec<usize> = self.store.light_presets.iter().enumerate().filter(|(_, p)| p.model == model).map(|(i, _)| i).collect();
        let names: Vec<String> = idx.iter().map(|&i| self.store.light_presets[i].name.clone()).collect();
        let active = idx.iter().position(|&i| same_look(&self.store.light_presets[i].light, &self.light));
        let Some(action) = theme::presets_row(ui, &names, active, &mut self.light_preset_name) else { return };
        match action {
            PresetAction::Apply(n) => {
                let mut l = self.store.light_presets[idx[n]].light.clone();
                l.sent = self.light.sent;
                l.keys.resize(SLOTS, [255, 255, 255]);
                if l.keys != self.light.keys {
                    self.push_light_undo(self.light.keys.clone());
                }
                self.light = l;
                // light_live sends it and saves it as the current lighting.
                self.light_due = Some(Instant::now());
                self.set_status(format!("Lighting preset '{}' applied", names[n]), false);
                return;
            }
            PresetAction::Save(name) => {
                let p = LightPreset1 { name: name.clone(), model, light: self.light.clone() };
                match self.store.light_presets.iter_mut().find(|q| q.model == model && q.name == name) {
                    Some(q) => *q = p,
                    None => self.store.light_presets.push(p),
                }
                self.set_status(format!("Lighting preset '{name}' saved"), false);
            }
            PresetAction::Replace(n) => {
                self.store.light_presets[idx[n]].light = self.light.clone();
                self.set_status(format!("Lighting preset '{}' replaced with the current lighting", names[n]), false);
            }
            PresetAction::Delete(n) => {
                self.store.light_presets.remove(idx[n]);
                self.set_status(format!("Lighting preset '{}' deleted", names[n]), false);
            }
        }
        self.persist();
    }

    /// Custom light: brush, fill, undo and generated patterns.
    fn light_custom_tools(&mut self, ui: &mut egui::Ui) {
        const SWATCHES: [[u8; 3]; 9] = [
            [255, 0, 0],
            [255, 120, 0],
            [255, 220, 0],
            [0, 255, 0],
            [0, 220, 255],
            [0, 60, 255],
            [170, 0, 255],
            [255, 255, 255],
            [0, 0, 0],
        ];
        ui.horizontal_wrapped(|ui| {
            if ui.button("Fill all keys").clicked() {
                let all = vec![Some(self.light_paint); self.layout.len()];
                self.set_light_keys(&all);
            }
            if ui.add_enabled(!self.light_undo.is_empty(), egui::Button::new("Undo")).clicked() {
                if let Some(prev) = self.light_undo.pop() {
                    self.light.keys = prev;
                    self.light_due = Some(Instant::now());
                }
            }
            ui.separator();
            ui.label("Brush");
            ui.color_edit_button_srgb(&mut self.light_paint);
            for c in SWATCHES {
                let (r, p) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                ui.painter().rect_filled(r, 4.0, Color32::from_rgb(c[0], c[1], c[2]));
                let stroke = if self.light_paint == c { Stroke::new(2.0_f32, pal().text) } else { Stroke::new(1.0_f32, pal().disabled) };
                ui.painter().rect_stroke(r, 4.0, stroke, StrokeKind::Inside);
                if p.clicked() {
                    self.light_paint = c;
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Pattern");
            egui::ComboBox::from_id_salt("g1pattern_dir").selected_text(self.light_dir.label()).show_ui(ui, |ui| {
                for d in pattern::Direction::ALL {
                    ui.selectable_value(&mut self.light_dir, d, d.label());
                }
            });
            let centres: Vec<[f32; 2]> = self.layout.iter().map(|k| [k.x + k.w / 2.0, k.y + k.h / 2.0]).collect();
            let mut generated = None;
            if ui.button("Gradient").on_hover_text("Brush colour to the second colour").clicked() {
                generated = Some(pattern::gradient(&centres, self.light_dir, self.light_paint, self.light_paint2));
            }
            ui.color_edit_button_srgb(&mut self.light_paint2).on_hover_text("Second colour (gradient, stripes, checker)");
            if ui.button("Stripes").clicked() {
                generated = Some(pattern::stripes(&centres, self.light_dir, self.light_paint, self.light_paint2));
            }
            if ui.button("Checker").clicked() {
                generated = Some(pattern::checker(&centres, self.light_paint, self.light_paint2));
            }
            ui.separator();
            if ui.button("Rainbow").clicked() {
                generated = Some(pattern::rainbow(&centres, self.light_dir, 5.0 / 6.0, 0.0));
            }
            if ui.button("Random").clicked() {
                let seed = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
                generated = Some(pattern::random(centres.len(), seed));
            }
            ui.separator();
            let name = |i: usize| pattern::PRESETS.get(i).map_or("?", |p| p.0);
            egui::ComboBox::from_id_salt("g1palette").selected_text(name(self.light_palette)).show_ui(ui, |ui| {
                for i in 0..pattern::PRESETS.len() {
                    ui.selectable_value(&mut self.light_palette, i, name(i));
                }
            });
            if ui.button("Apply palette").clicked() {
                if let Some((_, stops)) = pattern::PRESETS.get(self.light_palette) {
                    generated = Some(pattern::palette(&centres, self.light_dir, stops));
                }
            }
            if let Some(colors) = generated {
                let per_key: Vec<_> = colors.into_iter().map(Some).collect();
                self.set_light_keys(&per_key);
            }
        });
        theme::help(ui, "Click or drag across keys to paint, right-click a key to pick its colour. Undo steps back one change.");
    }

    fn push_light_undo(&mut self, keys: Vec<[u8; 3]>) {
        self.light_undo.push(keys);
        if self.light_undo.len() > 30 {
            self.light_undo.remove(0);
        }
    }

    /// Sets per-key colours by layout index (`None` = leave) as one undo step, sent live.
    fn set_light_keys(&mut self, per_key: &[Option<[u8; 3]>]) {
        let before = self.light.keys.clone();
        for (k, c) in self.layout.iter().zip(per_key) {
            if let (Some(c), Some(dst)) = (c, self.light.keys.get_mut(k.slot as usize)) {
                *dst = *c;
            }
        }
        if self.light.keys != before {
            self.push_light_undo(before);
            self.light_due = Some(Instant::now());
        }
    }

    /// Brush stroke on the keyboard: starts with a primary press on it (not
    /// under a popup), paints every key crossed until release.
    fn light_paint_stroke(&mut self, ui: &egui::Ui, area: Rect, keys: &[KeyDef], origin: Pos2, unit: f32, gap: f32) {
        let (pos, down, pressed) = ui.input(|i| (i.pointer.latest_pos(), i.pointer.primary_down(), i.pointer.primary_pressed()));
        if self.light_stroke.is_none() && pressed && ui.rect_contains_pointer(area) {
            self.push_light_undo(self.light.keys.clone());
            self.light_stroke = pos;
        }
        let Some(prev) = self.light_stroke else { return };
        if let Some(p) = pos {
            let steps = ((p - prev).length() / (unit * 0.2)).ceil().max(1.0) as usize;
            let mut changed = false;
            for n in 0..=steps {
                let q = prev + (p - prev) * (n as f32 / steps as f32);
                if let Some(k) = keys.iter().find(|k| key_rect(origin, unit, gap, k).contains(q)) {
                    if let Some(c) = self.light.keys.get_mut(k.slot as usize) {
                        if *c != self.light_paint {
                            *c = self.light_paint;
                            changed = true;
                        }
                    }
                }
            }
            if changed {
                self.light_due = Some(Instant::now());
            }
            self.light_stroke = Some(p);
        }
        if !down {
            self.light_stroke = None;
            if self.light_undo.last() == Some(&self.light.keys) {
                self.light_undo.pop();
            } else {
                self.store.set_lighting(self.model, &self.light);
                self.persist();
            }
        }
    }

    /// Sends pending lighting: at once for clicks and painting (also
    /// mid-stroke), a held slider after a short pause; one write in flight.
    fn light_live(&mut self, ctx: &egui::Context) {
        let Some(due) = self.light_due else { return };
        let now = Instant::now();
        let held = ctx.input(|i| i.pointer.primary_down());
        if self.busy || (held && self.light_stroke.is_none() && now < due) {
            ctx.request_repaint_after(due.saturating_duration_since(now).max(Duration::from_millis(30)));
            return;
        }
        self.light_due = None;
        self.light.sent = true;
        if !self.demo && self.identity.is_some() {
            let (packets, what) = if self.light.custom {
                let keys: Vec<(u8, [u8; 3])> =
                    self.layout.iter().map(|k| (k.slot, self.light.keys.get(k.slot as usize).copied().unwrap_or([0; 3]))).collect();
                (led::custom(&keys, self.light.bright), "Key colours")
            } else {
                (vec![led::effect(self.light.effect, self.light.speed, self.light.bright, self.light.colour)], "Lighting")
            };
            self.dev.send(Cmd::Lighting { packets, what });
            self.busy = true;
        }
        if self.light_stroke.is_none() {
            self.store.set_lighting(self.model, &self.light);
            self.persist();
        }
    }

    // ---------------------------------------------------------- switch rules

    fn pair_mode(&self) -> PairMode {
        if self.flags.last_win {
            PairMode::LastWin
        } else if self.flags.rdt {
            PairMode::Rdt
        } else {
            PairMode::Off
        }
    }

    /// Official rules: LW/RDT exclusive; either one forces Rapid Trigger on
    /// and Turbo off ("Turbo mode cannot be used under LW and RDT functions").
    fn set_pair_mode(&mut self, mode: PairMode) {
        self.flags.last_win = mode == PairMode::LastWin;
        self.flags.rdt = mode == PairMode::Rdt;
        if mode != PairMode::Off {
            self.flags.rapid_trigger = true;
            self.flags.turbo = false;
        }
        if self.pick.is_some_and(|(k, _)| k != mode) {
            self.pick = None;
        }
    }

    // ---------------------------------------------------------- UI

    /// Reload / Save and the unsaved count (header, left).
    fn actions(&mut self, ui: &mut egui::Ui) {
        let p = pal();
        let ready = self.connected_model().is_some() && !self.busy && !self.demo;
        let dirty = self.any_dirty();
        let unsaved = self.unsaved_count().max(dirty as usize);
        if ui
            .add_enabled(ready, egui::Button::new("Reload from keyboard"))
            .on_hover_text("Reads the keyboard again and replaces unsaved changes. The keymap and lighting stay as shown here (the keyboard can't report them).")
            .clicked()
        {
            self.dev.send(Cmd::Refresh);
        }
        if theme::primary_button(ui, ready && dirty, "Save to keyboard").clicked() {
            self.save_to_keyboard();
        }
        if self.busy {
            ui.spinner();
        }
        if unsaved > 0 {
            theme::dot(ui, p.unsaved);
            let text = if unsaved == 1 { "1 unsaved change".to_string() } else { format!("{unsaved} unsaved changes") };
            ui.label(egui::RichText::new(text).color(p.unsaved));
        }
    }

    /// Keyboard pill and profile (header, right-to-left). Returns the pill.
    fn device_info(&self, ui: &mut egui::Ui, switch: bool, compact: bool) -> egui::Response {
        let p = pal();
        let profile = match (&self.identity, self.slots, self.connected_model()) {
            (Some(id), Some(n), Some(_)) => Some(format!("Profile {} of {n}", id.profile_index + 1)),
            _ => None,
        };
        if let Some(t) = profile.as_ref().filter(|_| !compact) {
            ui.label(egui::RichText::new(t).color(p.muted));
        }
        let (dot, name, state) = match (&self.identity, self.connected_model()) {
            (Some(_), Some(m)) => (p.accent, m.name().to_string(), "Connected"),
            (Some(_), None) => (p.unsaved, "Unsupported keyboard".to_string(), ""),
            (None, _) => (p.disabled, "No keyboard found".to_string(), ""),
        };
        let mut tip: Vec<String> = Vec::new();
        if let Some(id) = &self.identity {
            tip.push(format!("Firmware {}", id.firmware_label));
        }
        if let Some(t) = profile.filter(|_| compact) {
            tip.push(t);
        }
        if switch {
            tip.push("Click to show the other keyboard.".into());
        }
        let r = theme::pill(ui, dot, &name, state, switch);
        if tip.is_empty() {
            r
        } else {
            r.on_hover_text(tip.join("\n"))
        }
    }

    fn unsaved_count(&self) -> usize {
        [self.travel_dirty(), self.flags_dirty(), self.pairs_dirty() || self.lw_replace_dirty(), self.match_dirty(), self.keymap_dirty || self.knob_dirty]
            .into_iter()
            .filter(|d| *d)
            .count()
    }

    /// Amber dot in the page menu.
    fn page_dirty(&self, page: Tab) -> bool {
        match page {
            Tab::Performance => self.travel_dirty() || self.flags_dirty() || self.match_dirty(),
            Tab::Keymap => self.keymap_dirty || self.knob_dirty,
            Tab::Advanced => self.pairs_dirty() || self.lw_replace_dirty(),
            _ => false,
        }
    }

    fn diagnostics(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Troubleshooting");
        theme::help(ui, "Turn on the detailed log before reproducing a problem, then send the log file with your report.");
        if ui.checkbox(&mut self.store.settings.log_packets, "Detailed log for bug reports").changed() {
            crate::log::set_packet_logging(self.store.settings.log_packets);
            self.persist();
        }
        if ui.button("Open log folder").clicked() {
            let _ = std::fs::create_dir_all(crate::log::dir());
            let _ = std::process::Command::new("explorer").arg(crate::log::dir()).spawn();
        }
    }

    /// Whole-keyboard switches (Performance page).
    fn keyboard_switches(&mut self, ui: &mut egui::Ui) {
        let rt = theme::switch_row(
            ui,
            &mut self.flags.rapid_trigger,
            "Rapid Trigger",
            "Resets a key as soon as it starts rising, so you can press again without letting go fully.",
        );
        if rt.changed() && !self.flags.rapid_trigger {
            self.set_pair_mode(PairMode::Off);
        }
        let turbo = theme::switch_row(ui, &mut self.flags.turbo, "Turbo", "Fastest response. Lighting stays still and key pairs turn off while Turbo is on.");
        if turbo.changed() && self.flags.turbo && self.pair_mode() != PairMode::Off {
            self.set_pair_mode(PairMode::Off);
            self.set_status("Turbo can't be used with Last Win / RDT — key pairs turned off", false);
        }
        let rtm = theme::switch_row(ui, &mut self.flags.rt_match, "RT Match", "Lets the keyboard tune Rapid Trigger for you.");
        if rtm.changed() && !self.flags.rt_match {
            // Official: turning RT Match off also clears the auto mode (255).
            self.match_mode = None;
        }
        if self.flags.rt_match {
            theme::setting(ui, "RT Match mode", "", |ui| {
                egui::ComboBox::from_id_salt("match_mode")
                    .selected_text(self.match_mode.map_or("Manual", MatchMode::name))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.match_mode, None, "Manual");
                        for m in MatchMode::ALL {
                            ui.selectable_value(&mut self.match_mode, Some(m), m.name()).on_hover_text(m.help());
                        }
                    });
            });
        }
    }

    /// Off / Last Win / RDT (Advanced keys page).
    fn pair_mode_picker(&mut self, ui: &mut egui::Ui) {
        let before = self.pair_mode();
        let mut mode = before;
        theme::setting(ui, "Key pairs", "Both modes need Rapid Trigger and turn Turbo off.", |ui| {
            ui.selectable_value(&mut mode, PairMode::Off, "Off");
            ui.selectable_value(&mut mode, PairMode::LastWin, "Last Win")
                .on_hover_text("When both keys of a pair are held, the last one pressed wins.");
            ui.selectable_value(&mut mode, PairMode::Rdt, "RDT")
                .on_hover_text("Release Dual-Trigger: releasing the press key triggers the release key.");
        });
        if mode != before {
            self.set_pair_mode(mode);
        }
        if self.flags.last_win {
            theme::switch_row(ui, &mut self.lw_replace, "Full replace", "Official 'Replace full action' option for Last Win.");
        }
    }

    fn slot_selector(&mut self, ui: &mut egui::Ui, n: u8) {
        theme::section(ui, "Profiles on the keyboard");
        let cur = self.identity.as_ref().map_or(0, |i| i.profile_index);
        let dirty = self.any_dirty();
        let can = !self.busy && !dirty && !self.demo && self.connected_model().is_some();
        ui.horizontal(|ui| {
            for i in 0..n {
                let b = egui::Button::new(format!(" {} ", i + 1)).selected(i == cur);
                if ui.add_enabled(can, b).clicked() && i != cur {
                    self.dev.send(Cmd::SelectSlot { index: i, count: n });
                }
            }
            ui.separator();
            if ui.add_enabled(can && n < rtp::MAX_SLOTS, egui::Button::new("+")).on_hover_text("Add a profile").clicked() {
                self.dev.send(Cmd::SetSlotCount(n + 1));
            }
            let can_remove = can && n > 1 && cur + 1 < n;
            if ui.add_enabled(can_remove, egui::Button::new("-")).on_hover_text("Remove the last profile").clicked() {
                self.dev.send(Cmd::SetSlotCount(n - 1));
            }
        });
        theme::help(ui, if dirty { "Save or reload first to switch profiles." } else { "Each profile keeps its own key settings on the keyboard." });
    }

    fn profiles(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Saved on this PC");
        theme::help(ui, "Copies of your settings kept on this computer. Load one, then press Save to keyboard.");
        let mut load = None;
        let mut delete = None;
        for (i, p) in self.store.profiles.iter().enumerate().filter(|(_, p)| p.model == self.model) {
            ui.horizontal(|ui| {
                if ui.button("Load").clicked() {
                    load = Some(i);
                }
                if ui.small_button("x").on_hover_text("Delete profile").clicked() {
                    delete = Some(i);
                }
                ui.label(&p.name);
            });
        }
        if let Some(i) = load {
            let p = self.store.profiles[i].clone();
            self.travel.ap.copy_from_slice(&p.ap);
            self.travel.ds.copy_from_slice(&p.ds);
            self.travel.us.copy_from_slice(&p.us);
            self.flags = p.flags;
            // Profiles from before pair support carry no tables: keep the current ones.
            if !p.lw.is_empty() {
                self.lw = p.lw;
            }
            if !p.rdt.is_empty() {
                self.rdt = p.rdt;
            }
            self.match_mode = p.match_mode;
            self.lw_replace = p.lw_replace;
            self.keymap_dirty |= self.keymap != p.keymap;
            self.keymap = p.keymap;
            self.set_status(format!("Loaded profile '{}' (not yet saved to keyboard)", p.name), false);
        }
        if let Some(i) = delete {
            self.store.profiles.remove(i);
            self.persist();
        }
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.new_profile_name).hint_text(theme::hint("New profile name")).desired_width(130.0));
            let name = self.new_profile_name.trim().to_string();
            if ui.add_enabled(!name.is_empty(), egui::Button::new("Save")).clicked() {
                let p = self.working_profile(name.clone());
                match self.store.profiles.iter_mut().find(|q| q.name == name && q.model == self.model) {
                    Some(q) => *q = p,
                    None => self.store.profiles.push(p),
                }
                self.new_profile_name.clear();
                self.persist();
                self.set_status(format!("Profile '{name}' saved"), false);
            }
        });
    }

    /// Everything on the gen-1 keyboard pages as shown now (unsaved edits included).
    fn working_profile(&self, name: String) -> Profile {
        Profile {
            name,
            model: self.model,
            ap: self.travel.ap.to_vec(),
            ds: self.travel.ds.to_vec(),
            us: self.travel.us.to_vec(),
            flags: self.flags,
            keymap: self.keymap.clone(),
            lw: self.lw.clone(),
            rdt: self.rdt.clone(),
            match_mode: self.match_mode,
            lw_replace: self.lw_replace,
        }
    }

    /// Badge shown on a key in the Pairs tab.
    fn pair_tag(&self, slot: u8) -> Option<String> {
        if self.flags.last_win {
            if let Some(i) = self.lw.iter().position(|&(a, b)| a == slot || b == slot) {
                return Some(format!("LW{}", i + 1));
            }
        }
        if self.flags.rdt {
            for (i, p) in self.rdt.iter().enumerate() {
                if p.main == slot {
                    return Some(format!("R{} press", i + 1));
                }
                if p.trigger == slot {
                    return Some(format!("R{} release", i + 1));
                }
            }
        }
        None
    }

    fn keyboard(&mut self, ui: &mut egui::Ui) {
        let keys = std::mem::take(&mut self.layout);
        let unit = theme::key_unit(ui);
        let size = Vec2::new(16.25 * unit, 6.5 * unit);
        let (resp, painter) = ui.allocate_painter(size, Sense::click());
        theme::plate(&painter, resp.rect);
        let origin = resp.rect.min;
        let gap = (unit * 0.06).max(2.0);
        let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command || i.modifiers.shift);
        let mut clicked = None;
        let mut right_clicked = None;

        // Knob (not a key). On the Keymap page it can be clicked to remap its six actions.
        let kc = origin + Vec2::new((KNOB_POS.0 + 0.5) * unit, (KNOB_POS.1 + 0.5) * unit);
        let knob_page = self.tab == Tab::Keymap;
        let knob_resp = knob_page.then(|| {
            ui.interact(Rect::from_center_size(kc, Vec2::splat(unit * 0.9)), ui.id().with("knob"), Sense::click())
                .on_hover_text("Knob: click to choose what turning and pressing it does")
        });
        let knob_ring = if knob_page && self.knob_event.is_some() {
            pal().accent
        } else if knob_page && self.knob.iter().any(Option::is_some) {
            pal().key_tag
        } else if knob_resp.as_ref().is_some_and(|r| r.hovered()) {
            pal().text
        } else {
            pal().key_top
        };
        painter.circle(kc, unit * 0.45, pal().key_skirt, Stroke::new(2.0_f32, knob_ring));
        if knob_resp.as_ref().is_some_and(|r| r.clicked()) {
            self.selected.clear();
            self.knob_event = Some(self.knob_event.unwrap_or(KnobEvent::RotateRight));
        }

        let factory_layer = self.factory.layers[self.layer as usize];
        let picked = self.pick.and_then(|(_, first)| first);
        let small = FontId::proportional(theme::key_font(unit, 0.22, 9.0, 13.0));
        let big = FontId::proportional(theme::key_font(unit, 0.26, 10.0, 15.0));
        let tiny = FontId::proportional(theme::key_font(unit, 0.18, 8.0, 11.0));
        for key in &keys {
            let rect = key_rect(origin, unit, gap, key);
            let id = ui.id().with(("key", key.slot));
            let r = ui.interact(rect, id, Sense::click());
            if r.clicked() {
                clicked = Some(key.slot);
            }
            if r.secondary_clicked() {
                right_clicked = Some(key.slot);
            }
            let slot = key.slot as usize;
            let (selected, accent) = match self.tab {
                Tab::Performance => (self.selected.contains(&key.slot), false),
                Tab::Keymap => (self.selected.contains(&key.slot), self.keymap.get(self.layer, slot) != factory_layer[slot]),
                Tab::Advanced => (picked == Some(key.slot), false),
                Tab::Lighting | Tab::Profiles | Tab::Settings => (false, false),
            };
            let tag = if self.tab == Tab::Advanced { self.pair_tag(key.slot) } else { None };
            let key_color = (self.tab == Tab::Lighting && self.light.custom).then(|| self.light.keys.get(slot).copied()).flatten();
            let edge = if accent || tag.is_some() { Some(pal().key_tag) } else { None };
            let cap = Cap { fill: key_color.map(|c| Color32::from_rgb(c[0], c[1], c[2])), selected, hovered: r.hovered(), edge, empty: false };
            // Text goes on the top face.
            let rect = theme::keycap(&painter, rect, cap);
            let pad = unit * 0.06;
            let live = (self.tab == Tab::Performance && self.live_depth).then(|| self.depth.as_ref().map(|d| d[slot])).flatten();
            if let Some(mm) = live {
                theme::depth_overlay(&painter, rect, mm, self.travel.ap[slot]);
            }
            match self.tab {
                Tab::Performance => {
                    let name = painter.text(rect.left_top() + Vec2::splat(pad), Align2::LEFT_TOP, key.label, small.clone(), pal().key_legend);
                    let ap = self.travel.ap[slot];
                    let value = painter.text(rect.left_bottom() + Vec2::new(pad, -pad), Align2::LEFT_BOTTOM, format!("{ap:.2}"), big.clone(), ap_color(ap));
                    let (ds, us) = (self.travel.ds[slot], self.travel.us[slot]);
                    let rt = self.flags.rapid_trigger;
                    // Press / release sensitivity between name and value, only where it fits;
                    // 0 = keyboard default, not shown. The tooltip always has all values.
                    if rt && (ds > 0.0 || us > 0.0) {
                        let fmt = |v: f32| if v > 0.0 { theme::mm(v) } else { "-".into() };
                        let space = Rect::from_x_y_ranges(name.left()..=rect.right() - pad, name.bottom() + 1.0..=value.top() - 1.0);
                        theme::key_text_if_fits(&painter, space, format!("{} {}", fmt(ds), fmt(us)), FontId::proportional(theme::key_font(unit, 0.16, 8.0, 11.0)), pal().muted);
                    }
                    let tip = if rt {
                        format!("{}: actuation {} mm, press {}, release {}", key.label, theme::mm(ap), theme::mm(ds), theme::mm(us))
                    } else {
                        format!("{}: actuation {} mm", key.label, theme::mm(ap))
                    };
                    let tip = match live {
                        Some(mm) => format!("{tip}\nNow pressed {} mm", theme::mm(mm)),
                        None => tip,
                    };
                    let _ = r.clone().on_hover_text(tip);
                }
                Tab::Keymap => {
                    painter.text(rect.left_top() + Vec2::splat(pad), Align2::LEFT_TOP, key.label, small.clone(), pal().muted);
                    let action = self.keymap.get(self.layer, slot);
                    let text = if action.is_none() { "—".to_string() } else { catalog::label(action) };
                    let color = if accent { pal().key_tag } else { pal().text };
                    painter.text(rect.center() + Vec2::new(0.0, unit * 0.12), Align2::CENTER_CENTER, text, big.clone(), color);
                }
                Tab::Profiles | Tab::Settings => {}
                Tab::Lighting => {
                    // Dark text on light keys.
                    let ink = key_color.map_or(pal().key_legend, |c| theme::ink_on(Color32::from_rgb(c[0], c[1], c[2])));
                    painter.text(rect.left_top() + Vec2::splat(pad), Align2::LEFT_TOP, key.label, small.clone(), ink);
                }
                Tab::Advanced => {
                    let label_color = if tag.is_some() || selected { pal().text } else { pal().muted };
                    painter.text(rect.left_top() + Vec2::splat(pad), Align2::LEFT_TOP, key.label, small.clone(), label_color);
                    if let Some(t) = tag {
                        painter.text(rect.left_bottom() + Vec2::new(pad, -pad), Align2::LEFT_BOTTOM, t, tiny.clone(), pal().key_tag);
                    }
                }
            }
        }
        if self.tab == Tab::Lighting && self.light.custom {
            if let Some(c) = right_clicked.and_then(|s| self.light.keys.get(s as usize)) {
                self.light_paint = *c;
            }
            self.light_paint_stroke(ui, resp.rect, &keys, origin, unit, gap);
        }
        self.layout = keys;
        match clicked {
            Some(slot) => self.click_key(slot, ctrl),
            None if resp.clicked() && !ctrl => self.selected.clear(),
            None => {}
        }
    }

    fn click_key(&mut self, slot: u8, additive: bool) {
        match (self.tab, additive) {
            (Tab::Advanced, _) => self.pick_key(slot),
            (Tab::Lighting | Tab::Profiles | Tab::Settings, _) => {}
            (Tab::Keymap, _) | (Tab::Performance, false) => {
                self.knob_event = None;
                let only_this = self.selected.len() == 1 && self.selected.contains(&slot);
                self.selected.clear();
                if !only_this {
                    self.selected.insert(slot);
                }
            }
            (Tab::Performance, true) => {
                if !self.selected.remove(&slot) {
                    self.selected.insert(slot);
                }
            }
        }
    }

    fn slot_paired(&self, kind: PairMode, slot: u8) -> bool {
        match kind {
            PairMode::LastWin => self.lw.iter().any(|&(a, b)| a == slot || b == slot),
            PairMode::Rdt => self.rdt.iter().any(|p| p.main == slot || p.trigger == slot),
            PairMode::Off => false,
        }
    }

    fn pick_key(&mut self, slot: u8) {
        let Some((kind, first)) = self.pick else { return };
        if first == Some(slot) {
            self.pick = Some((kind, None));
            return;
        }
        if self.slot_paired(kind, slot) {
            self.set_status(format!("{} is already in a pair", self.key_name(slot)), true);
            return;
        }
        match first {
            None => self.pick = Some((kind, Some(slot))),
            Some(a) => {
                match kind {
                    PairMode::LastWin => self.lw.push((a, slot)),
                    PairMode::Rdt => self.rdt.push(RdtPair::new(a, slot)),
                    PairMode::Off => {}
                }
                self.pick = None;
            }
        }
    }

    fn performance_editor(&mut self, ui: &mut egui::Ui) {
        if ui.available_width() > 860.0 {
            ui.columns(2, |c| {
                theme::card(&mut c[0], |ui| self.travel_card(ui));
                theme::card(&mut c[1], |ui| self.switches_card(ui));
            });
        } else {
            theme::card(ui, |ui| self.travel_card(ui));
            theme::card(ui, |ui| self.switches_card(ui));
        }
    }

    fn switches_card(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Whole keyboard");
        theme::help(ui, "These apply to every key.");
        ui.separator();
        self.keyboard_switches(ui);
        ui.separator();
        self.live_depth_switch(ui);
    }

    /// Turbo as the keyboard has it now (the official driver doesn't track depth with Turbo on).
    fn turbo_on_keyboard(&self) -> bool {
        if self.demo {
            self.flags.turbo
        } else {
            self.identity.as_ref().is_some_and(|i| i.turbo)
        }
    }

    fn live_depth_switch(&mut self, ui: &mut egui::Ui) {
        let turbo = self.turbo_on_keyboard();
        ui.add_enabled_ui(!turbo, |ui| {
            theme::switch_row(
                ui,
                &mut self.live_depth,
                "Show live key depth",
                "Press keys on your keyboard to see how far each one goes down. The shading changes colour once a key passes its actuation point (the line across it).",
            );
        });
        if turbo {
            theme::help(ui, "Live key depth needs Turbo off (saved to the keyboard).");
        }
    }

    /// Starts or stops depth tracking on the device thread to match the page.
    fn sync_tracking(&mut self, ctx: &egui::Context) {
        let want = self.live_depth && self.tab == Tab::Performance && self.board == Board::Gen1 && !self.turbo_on_keyboard();
        if self.demo {
            self.demo_depth(want, ctx);
            return;
        }
        let want = want && self.identity.as_ref().is_some_and(|i| i.model.is_some());
        if want != self.tracking_sent {
            self.dev.send(Cmd::Tracking(want));
            self.tracking_sent = want;
            if !want {
                self.depth = None;
            }
        }
    }

    /// Demo mode: W, A, S and D move up and down so the view can be checked without a keyboard.
    fn demo_depth(&mut self, on: bool, ctx: &egui::Context) {
        if !on {
            self.depth = None;
            return;
        }
        let t = self.started.elapsed().as_secs_f32();
        let mut d = [0f32; SLOTS];
        for (i, name) in ["W", "A", "S", "D"].iter().enumerate() {
            if let Some(k) = self.layout.iter().find(|k| k.label == *name) {
                let phase = t * 1.3 + i as f32 * 1.7;
                d[k.slot as usize] = ((phase.sin() * 0.5 + 0.5) * 3.6 - 0.3).clamp(0.0, 3.3);
            }
        }
        self.depth = Some(Box::new(d));
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }

    fn travel_card(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            theme::section(ui, "Selected keys");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("None").clicked() {
                    self.selected.clear();
                }
                if ui.button("WASD").clicked() {
                    self.selected = [44u8, 64, 65, 66].into_iter().collect();
                }
                if ui.button("All").clicked() {
                    self.selected = self.layout.iter().map(|k| k.slot).collect();
                }
            });
        });
        if self.selected.is_empty() {
            theme::help(ui, "Click a key on the keyboard to change it. Ctrl+click adds more keys.");
            return;
        }
        let names: Vec<&str> = self.selected.iter().take(6).map(|&s| self.key_name(s)).collect();
        let more = self.selected.len().saturating_sub(names.len());
        let list = if more > 0 { format!("{} and {more} more", names.join(", ")) } else { names.join(", ") };
        theme::help(ui, format!("{list}. Ctrl+click adds more keys."));
        ui.separator();
        let sel: Vec<usize> = self.selected.iter().map(|&s| s as usize).collect();
        let avg = |v: &Values| sel.iter().map(|&s| v[s]).sum::<f32>() / sel.len() as f32;
        let mixed = |v: &Values| sel.iter().any(|&s| (v[s] - v[sel[0]]).abs() > 0.004);
        let w = ui.available_width();
        ui.spacing_mut().slider_width = (w - (w * 0.42).clamp(170.0, 340.0) - 110.0).clamp(110.0, 360.0);
        for (kind, name, help) in [
            (Travel::Ap, "Actuation point", "How far down the key goes before it registers. Lower is faster; higher avoids accidental presses."),
            (Travel::Ds, "Press sensitivity", "Rapid Trigger: how far you press again to re-trigger the key."),
            (Travel::Us, "Release sensitivity", "Rapid Trigger: how far the key rises before it resets."),
        ] {
            // DS/US 0 = keyboard default (official range 0..3.1).
            let (lo, hi) = kind.range_mm();
            let values = match kind {
                Travel::Ap => &mut self.travel.ap,
                Travel::Ds => &mut self.travel.ds,
                Travel::Us => &mut self.travel.us,
            };
            let mut v = avg(values);
            let label = if mixed(values) { format!("{name} (mixed)") } else { name.to_string() };
            theme::setting(ui, &label, help, |ui| {
                let slider = egui::Slider::new(&mut v, lo..=hi).step_by(0.01);
                let slider = if kind == Travel::Ap {
                    slider.fixed_decimals(2).suffix(" mm")
                } else {
                    slider
                        .custom_formatter(|v, _| if v <= 0.0 { "default".into() } else { format!("{v:.2} mm") })
                        .custom_parser(|t| {
                            let t = t.trim().trim_end_matches("mm").trim();
                            if t.eq_ignore_ascii_case("default") {
                                Some(0.0)
                            } else {
                                t.parse().ok()
                            }
                        })
                };
                if ui.add(slider).changed() {
                    for &s in &sel {
                        values[s] = v;
                    }
                }
            });
        }
        if !self.flags.rapid_trigger {
            theme::help(ui, "Rapid Trigger is off, so the two sensitivity values are not used yet.");
        }
    }

    fn remap_editor(&mut self, ui: &mut egui::Ui) {
        if let Some(ev) = self.knob_event {
            self.knob_editor(ui, ev);
            return;
        }
        ui.horizontal(|ui| {
            ui.label("Layer:");
            for l in Layer::ALL {
                ui.selectable_value(&mut self.layer, l, l.name());
            }
            ui.separator();
            if ui.button(format!("Reset {} layer to factory", self.layer.name())).clicked() {
                self.keymap.layers[self.layer as usize] = self.factory.layers[self.layer as usize];
                self.keymap_dirty = true;
            }
        });
        theme::help(ui, "The keymap can't be read from the keyboard; Save writes all three layers exactly as shown here.");
        ui.separator();
        let Some(&slot) = self.selected.iter().next() else {
            theme::help(ui, "Click a key on the keyboard, then pick what it should send. Click the knob to set what it does.");
            return;
        };
        let s = slot as usize;
        let factory = self.factory.get(self.layer, s);
        let current = self.keymap.get(self.layer, s);
        let name = self.key_name(slot);
        ui.horizontal(|ui| {
            ui.strong(format!("{name} · {} layer", self.layer.name()));
            ui.label(format!(
                "sends: {}   (factory: {})",
                none_dash(catalog::label(current)),
                none_dash(catalog::label(factory))
            ));
            if ui.add_enabled(current != factory, egui::Button::new("Restore")).clicked() {
                self.keymap.set(self.layer, s, factory);
                self.keymap_dirty = true;
            }
            if ui.add_enabled(!current.is_none(), egui::Button::new("Clear")).clicked() {
                self.keymap.set(self.layer, s, KeyAction::NONE);
                self.keymap_dirty = true;
            }
        });
        // The page scrolls; no inner scroll area, so the keyboard keeps its size.
        ui.scope(|ui| {
            for cat in CATEGORIES {
                egui::CollapsingHeader::new(cat.name).default_open(true).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for item in cat.items {
                            let on = item.action == current;
                            if ui.add(egui::Button::new(item.label).selected(on).min_size(Vec2::new(40.0, 24.0))).clicked() {
                                self.keymap.set(self.layer, s, item.action);
                                self.keymap_dirty = true;
                            }
                        }
                    });
                });
            }
        });
    }

    fn knob_editor(&mut self, ui: &mut egui::Ui, ev: KnobEvent) {
        if !self.knob_supported() {
            theme::help(ui, "Knob remapping needs keyboard firmware 0.063 or newer.");
            return;
        }
        ui.horizontal_wrapped(|ui| {
            ui.strong("Knob");
            for e in KnobEvent::ALL {
                let mapped = self.knob[e.index()].is_some();
                let text = if mapped { format!("{} •", e.name()) } else { e.name().to_string() };
                if ui.selectable_label(e == ev, text).clicked() {
                    self.knob_event = Some(e);
                }
            }
        });
        let i = ev.index();
        let current = self.knob[i];
        ui.horizontal(|ui| {
            ui.label(format!("{}: {}", ev.name(), current.map_or("knob's own action".to_string(), catalog::label)));
            if ui.add_enabled(current.is_some(), egui::Button::new("Use knob's own action")).clicked() {
                self.knob[i] = None;
                self.knob_dirty = true;
            }
            if ui.add_enabled(self.knob.iter().any(Option::is_some), egui::Button::new("Reset all knob actions")).clicked() {
                self.knob = [None; 6];
                self.knob_dirty = true;
            }
        });
        theme::help(ui, "The knob works the same on every layer. It can't be read from the keyboard; Save writes all six knob actions as shown here. Click a key to go back to the keys.");
        ui.separator();
        ui.scope(|ui| {
            for cat in CATEGORIES {
                egui::CollapsingHeader::new(cat.name).id_salt(("knob", cat.name)).default_open(true).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for item in cat.items {
                            let on = Some(item.action) == current;
                            if ui.add(egui::Button::new(item.label).selected(on).min_size(Vec2::new(40.0, 24.0))).clicked() {
                                self.knob[i] = Some(item.action);
                                self.knob_dirty = true;
                            }
                        }
                    });
                });
            }
        });
    }

    fn pairs_editor(&mut self, ui: &mut egui::Ui) {
        let mode = self.pair_mode();
        let (title, doc, count, max) = match mode {
            PairMode::Off => {
                ui.label("Choose Last Win or RDT above to edit pairs.");
                theme::help(ui, "Both need Rapid Trigger and can't be combined with Turbo.");
                return;
            }
            PairMode::LastWin => (
                "Last Win",
                "When both keys of a pair are held, the last one pressed wins.",
                self.lw.len(),
                rtp::MAX_LW_PAIRS,
            ),
            PairMode::Rdt => (
                "Release Dual-Trigger",
                "Releasing the press key triggers the release key.",
                self.rdt.len(),
                rtp::MAX_RDT_PAIRS,
            ),
        };
        ui.horizontal(|ui| {
            ui.strong(title);
            ui.label(doc);
        });
        ui.horizontal(|ui| match self.pick {
            Some((_, first)) => {
                match first {
                    None => ui.label("Click the first key on the keyboard above."),
                    Some(a) => ui.label(format!("{} picked — now click the second key.", self.key_name(a))),
                };
                if ui.button("Cancel").clicked() {
                    self.pick = None;
                }
            }
            None => {
                if ui.add_enabled(count < max, egui::Button::new("Add pair")).clicked() {
                    self.pick = Some((mode, None));
                }
                theme::help(ui, format!("{count}/{max} pairs"));
            }
        });
        ui.separator();
        if count == 0 {
            ui.label("No pairs yet.");
            return;
        }
        let mut remove = None;
        let layout = &self.layout;
        let name = |s: u8| layout.iter().find(|k| k.slot == s).map(|k| k.label).unwrap_or("?");
        ui.scope(|ui| match mode {
            PairMode::LastWin => {
                for (i, &(a, b)) in self.lw.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("LW{}", i + 1)).color(pal().key_tag));
                        ui.strong(format!("{} + {}", name(a), name(b)));
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                }
            }
            PairMode::Rdt => {
                ui.spacing_mut().slider_width = 220.0;
                for (i, p) in self.rdt.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("R{}", i + 1)).color(pal().key_tag));
                        ui.strong(format!("press {} -> release {}", name(p.main), name(p.trigger)));
                        if ui.small_button("Remove").clicked() {
                            remove = Some(i);
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.add_space(24.0);
                        let (lo, hi) = RdtPair::ACTIVE_RANGE;
                        ui.add_sized([110.0, 18.0], egui::Label::new("Trigger"));
                        ui.add(egui::Slider::new(&mut p.active_mm, lo..=hi).step_by(0.01).fixed_decimals(2).suffix(" mm"));
                        let (lo, hi) = RdtPair::RESET_RANGE;
                        ui.add_sized([80.0, 18.0], egui::Label::new("Reset"));
                        ui.add(egui::Slider::new(&mut p.reset_mm, lo..=hi).step_by(0.01).fixed_decimals(2).suffix(" mm"));
                    });
                    ui.add_space(4.0);
                }
            }
            PairMode::Off => {}
        });
        if let Some(i) = remove {
            match mode {
                PairMode::LastWin => {
                    self.lw.remove(i);
                }
                PairMode::Rdt => {
                    self.rdt.remove(i);
                }
                PairMode::Off => {}
            }
        }
    }

    // ---------------------------------------------------------- Apps tab

    /// Agent / autostart state and agent.json edits from the tray, at most once a second.
    fn refresh_agent_status(&mut self) {
        if self.agent_checked.is_some_and(|t| t.elapsed() < Duration::from_secs(1)) {
            return;
        }
        self.agent_checked = Some(Instant::now());
        self.agent_running = sys::agent_running();
        self.autostart = sys::autostart_enabled();
        let stamp = AgentConfig::stamp();
        if stamp != self.agent_cfg_stamp {
            self.agent_cfg = AgentConfig::load();
            self.agent_cfg_stamp = stamp;
        }
    }

    fn start_agent(&mut self) {
        match sys::spawn_self(&["--agent"]) {
            Ok(()) => self.set_status("Tray agent started", false),
            Err(e) => self.set_status(format!("Could not start tray agent: {e}"), true),
        }
        // Its window appears a moment later; re-check on the next tick.
        self.agent_checked = Some(Instant::now() - Duration::from_millis(700));
    }

    fn save_agent_cfg(&mut self) {
        match self.agent_cfg.save() {
            Ok(()) => self.agent_cfg_stamp = AgentConfig::stamp(),
            Err(e) => self.set_status(format!("Could not save agent settings: {e}"), true),
        }
        if self.agent_cfg.auto_switch && !self.agent_running && !self.demo {
            self.start_agent();
        }
    }

    /// Tray agent and Start with Windows (Settings page, gen-1).
    fn tray_settings(&mut self, ui: &mut egui::Ui) {
        self.refresh_agent_status();
        if self.running_apps.is_empty() {
            self.running_apps = sys::running_apps();
        }
        ui.ctx().request_repaint_after(Duration::from_secs(1));

        theme::section(ui, "Tray and startup");
        ui.horizontal(|ui| {
            let (dot, text) = if self.agent_running {
                (pal().accent, "Running in the tray")
            } else {
                (pal().disabled, "Not running")
            };
            let (r, p) = ui.allocate_painter(Vec2::splat(14.0), Sense::hover());
            p.circle_filled(r.rect.center(), 5.0, dot);
            ui.label(text);
            if self.agent_running {
                if ui.button("Stop").clicked() {
                    sys::stop_agent();
                    self.agent_checked = Some(Instant::now() - Duration::from_millis(700));
                }
            } else if ui.add_enabled(!self.demo, egui::Button::new("Start")).clicked() {
                self.start_agent();
            }
        });
        theme::help(ui, "Runs without a window: a tray icon with a profile menu. It opens the keyboard only to switch profiles.");
        let mut autostart = self.autostart;
        if ui.checkbox(&mut autostart, "Start with Windows").on_hover_text("Starts the tray agent at login (HKCU Run).").changed() {
            match sys::set_autostart(autostart) {
                Ok(()) => {
                    self.autostart = autostart;
                    if autostart && !self.agent_running {
                        self.start_agent();
                    }
                }
                Err(e) => self.set_status(format!("Start with Windows: {e}"), true),
            }
        }

    }

    /// Per-app profile rules (Profiles page, gen-1).
    fn per_app(&mut self, ui: &mut egui::Ui) {
        self.refresh_agent_status();
        if self.running_apps.is_empty() {
            self.running_apps = sys::running_apps();
        }
        ui.ctx().request_repaint_after(Duration::from_secs(1));
        theme::section(ui, "Switch profile per app");
        theme::help(ui, "The tray agent switches to the profile you pick whenever that app comes to the front.");
        let count = self.slots.unwrap_or(rtp::MAX_SLOTS);
        if self.slots.is_none() {
            theme::help(ui, "Profile count unknown (keyboard not connected, or firmware older than 0.081).");
        }
        let mut changed = ui.checkbox(&mut self.agent_cfg.auto_switch, "Switch profile to match the app in front").changed();
        ui.horizontal(|ui| {
            ui.label("Apps without a rule:");
            egui::ComboBox::from_id_salt("default_slot")
                .selected_text(slot_text(self.agent_cfg.default_slot))
                .show_ui(ui, |ui| {
                    changed |= ui.selectable_value(&mut self.agent_cfg.default_slot, None, slot_text(None)).changed();
                    for i in 0..count {
                        changed |= ui.selectable_value(&mut self.agent_cfg.default_slot, Some(i), slot_text(Some(i))).changed();
                    }
                });
        });

        ui.add_space(6.0);
        let mut remove = None;
        egui::Grid::new("rules").num_columns(3).spacing([16.0, 6.0]).striped(true).show(ui, |ui| {
            for (i, r) in self.agent_cfg.rules.iter_mut().enumerate() {
                ui.label(&r.exe);
                egui::ComboBox::from_id_salt(("rule", i)).selected_text(slot_text(Some(r.slot))).show_ui(ui, |ui| {
                    for s in 0..count.max(r.slot + 1) {
                        changed |= ui.selectable_value(&mut r.slot, s, slot_text(Some(s))).changed();
                    }
                });
                if ui.small_button("Remove").clicked() {
                    remove = Some(i);
                }
                ui.end_row();
            }
        });
        if let Some(i) = remove {
            self.agent_cfg.rules.remove(i);
            changed = true;
        }

        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("running").selected_text("Running apps").width(180.0).show_ui(ui, |ui| {
                for app in &self.running_apps {
                    if ui.selectable_label(self.new_rule_exe == *app, app).clicked() {
                        self.new_rule_exe = app.clone();
                    }
                }
            });
            if ui.small_button("Refresh").clicked() {
                self.running_apps = sys::running_apps();
            }
            ui.add(egui::TextEdit::singleline(&mut self.new_rule_exe).hint_text(theme::hint("game.exe")).desired_width(170.0));
            egui::ComboBox::from_id_salt("new_rule_slot").selected_text(slot_text(Some(self.new_rule_slot))).show_ui(ui, |ui| {
                for s in 0..count {
                    ui.selectable_value(&mut self.new_rule_slot, s, slot_text(Some(s)));
                }
            });
            let exe = store::normalize_exe(&self.new_rule_exe);
            let dup = self.agent_cfg.rules.iter().any(|r| r.exe == exe);
            if ui.add_enabled(!exe.is_empty() && !dup, egui::Button::new("Add rule")).clicked() {
                self.agent_cfg.rules.push(AppRule { exe, slot: self.new_rule_slot });
                self.new_rule_exe.clear();
                changed = true;
            }
        });
        if changed {
            self.save_agent_cfg();
        }
        ui.add_space(6.0);
        theme::help(ui, "Kraken Control itself never switches profiles; only the tray agent does.");
        if self.agent_cfg.auto_switch && !self.agent_running {
            theme::note(ui, "Per-app switching needs the tray agent running (Settings › Tray and startup).");
        }
    }
}

fn slot_text(slot: Option<u8>) -> String {
    match slot {
        Some(i) => format!("Profile {}", i + 1),
        None => "Keep current profile".into(),
    }
}

impl App {
    /// Follows whichever board is connected; with both, keeps the user's choice.
    fn pick_board(&mut self) {
        let g1 = self.connected_model().is_some() || (self.demo && self.board == Board::Gen1);
        let g2 = self.gen2.connected.is_some();
        match self.board {
            Board::Gen1 if !g1 && g2 => self.board = Board::Gen2,
            Board::Gen2 if !g2 && g1 => self.board = Board::Gen1,
            _ => {}
        }
    }

}

impl App {
    fn set_tab(&mut self, page: Tab) {
        if page == Tab::Keymap && self.selected.len() > 1 {
            let first = *self.selected.iter().next().unwrap();
            self.selected = [first].into_iter().collect();
        }
        self.tab = page;
        self.pick = None;
    }

    fn page_blurb(&self) -> &'static str {
        match self.tab {
            Tab::Performance => "Choose how far each key travels before it registers, and how quickly it resets.",
            Tab::Keymap => "Change what each key sends. Click a key, then pick its new action.",
            Tab::Advanced => "Pair two keys: the last one pressed wins, or releasing one triggers the other.",
            Tab::Lighting => "Changes show on the keyboard straight away. No need to save.",
            Tab::Profiles => "Switch between profiles on the keyboard, keep copies on this PC, and switch automatically per app.",
            Tab::Settings => "Look of the app, the tray icon and troubleshooting.",
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.set_min_height(34.0);
            // Narrow window: the profile moves into the keyboard tooltip.
            let compact = ui.available_width() < 1100.0;
            let gen2 = self.board == Board::Gen2;
            if gen2 {
                self.gen2.actions(ui);
            } else {
                self.actions(ui);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // With both keyboards plugged in, the pill switches between them.
                let both = self.connected_model().is_some() && self.gen2.connected.is_some();
                let pill = if gen2 { self.gen2.device_info(ui, both, compact) } else { self.device_info(ui, both, compact) };
                if both && pill.clicked() {
                    self.board = if gen2 { Board::Gen1 } else { Board::Gen2 };
                }
            });
        });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        let (text, err) = if self.board == Board::Gen2 { self.gen2.status() } else { (self.status.as_str(), self.status_is_error) };
        ui.horizontal(|ui| theme::status_line(ui, text, err));
    }

    /// Appearance and troubleshooting cards (Settings page, both boards).
    fn common_settings(&mut self, ui: &mut egui::Ui) {
        theme::card(ui, |ui| {
            theme::section(ui, "Appearance");
            let mut id = theme::current();
            theme::setting(
                ui,
                "Theme",
                "Dark Grey is the default. The Colour-blind themes keep every state apart with any type of colour blindness and mark selected keys with a thick outline.",
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for t in ThemeId::ALL {
                            ui.selectable_value(&mut id, t, t.name());
                        }
                    });
                },
            );
            let mut font = self.store.settings.font;
            theme::setting(ui, "Font", "Free, open-licensed fonts, each sized to fit the same space. Theme's font follows the theme.", |ui| {
                let theme_font = id.font().name();
                let text = font.map_or(format!("Theme's font ({theme_font})"), |f| f.name().to_string());
                egui::ComboBox::from_id_salt("ui_font").width(240.0).selected_text(text).show_ui(ui, |ui| {
                    ui.selectable_value(&mut font, None, format!("Theme's font ({theme_font})"));
                    for f in UiFont::ALL {
                        ui.selectable_value(&mut font, Some(f), f.name());
                    }
                });
            });
            if id != theme::current() || font != self.store.settings.font {
                theme::apply(ui.ctx(), id, font);
                ui.ctx().request_repaint();
                self.store.settings.theme = id;
                self.store.settings.font = font;
                self.persist();
            }
        });
        theme::card(ui, |ui| self.renderer_setting(ui));
        theme::card(ui, |ui| {
            self.diagnostics(ui);
            ui.add_space(4.0);
            let atlas = ui.ctx().fonts(|f| f.font_image_size());
            theme::help(ui, format!("Memory: {}", crate::mem::summary(atlas)));
            ui.ctx().request_repaint_after(Duration::from_secs(1));
        });
    }

    /// Software / GPU drawing (Settings › Performance); applies at the next start.
    fn renderer_setting(&mut self, ui: &mut egui::Ui) {
        use store::Renderer;
        theme::section(ui, "Drawing");
        let mut r = self.store.settings.renderer;
        theme::setting(
            ui,
            "Draw the window with",
            "Processor uses far less memory: the graphics driver isn't loaded. Graphics card can feel smoother on very large screens.",
            |ui| {
                ui.selectable_value(&mut r, Renderer::Software, "Processor (low memory)");
                ui.selectable_value(&mut r, Renderer::Gpu, "Graphics card");
            },
        );
        if r != self.store.settings.renderer {
            self.store.settings.renderer = r;
            self.persist();
        }
        let running = if crate::soft::active() { Renderer::Software } else { Renderer::Gpu };
        if r != running {
            ui.horizontal(|ui| {
                theme::note(ui, "Takes effect when Kraken Control restarts.");
                if ui.button("Restart now").clicked() {
                    match sys::spawn_self(&["--restarted"]) {
                        Ok(()) => std::process::exit(0),
                        Err(e) => self.set_status(format!("Could not restart: {e}"), true),
                    }
                }
            });
        }
    }

    /// Memory line in the log 10 s after start, then every 30 minutes.
    fn log_memory(&mut self, ctx: &egui::Context) {
        let due = match self.mem_logged {
            None => self.started.elapsed() >= Duration::from_secs(10),
            Some(t) => t.elapsed() >= Duration::from_secs(30 * 60),
        };
        if due {
            self.mem_logged = Some(Instant::now());
            let atlas = ctx.fonts(|f| f.font_image_size());
            crate::log::line(format!("memory: {}", crate::mem::summary(atlas)));
        }
    }

    fn keyboard_legend(&self) -> Option<&'static str> {
        match self.tab {
            Tab::Performance if self.live_depth && self.depth.is_some() => {
                Some("Live key depth: the shading shows how far each key is pressed and changes colour past its actuation point (the line across the key).")
            }
            Tab::Performance if self.flags.rapid_trigger => {
                Some("Big number: actuation point in mm. Small pair: press and release sensitivity (- = keyboard default). Ctrl+click selects more keys.")
            }
            Tab::Performance => Some("Numbers show the actuation point in mm. Ctrl+click selects more keys."),
            Tab::Keymap => Some("Keys show what they send on this layer. Outlined keys differ from the factory layout."),
            Tab::Advanced => Some("Outlined keys belong to a pair."),
            _ => None,
        }
    }

    fn gen1_page(&mut self, ui: &mut egui::Ui) {
        let (title, blurb) = (self.tab.title(), self.page_blurb());
        if self.tab == Tab::Lighting && (self.identity.is_some() || self.demo) {
            // Presets sit beside the title so the keyboard doesn't move down.
            theme::page_header_with(ui, title, blurb, |ui| self.light_presets(ui));
        } else {
            theme::page_header(ui, title, blurb);
        }
        if self.identity.is_none() && !self.demo && !matches!(self.tab, Tab::Profiles | Tab::Settings) {
            theme::no_keyboard(ui);
            return;
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.gen1_body(ui));
    }

    fn gen1_body(&mut self, ui: &mut egui::Ui) {
        match self.tab {
            Tab::Profiles => {
                if let Some(n) = self.slots {
                    theme::card(ui, |ui| self.slot_selector(ui, n));
                }
                theme::card(ui, |ui| self.profiles(ui));
                theme::card(ui, |ui| self.per_app(ui));
                theme::card(ui, |ui| self.backup_card(ui));
                return;
            }
            Tab::Settings => {
                self.common_settings(ui);
                theme::card(ui, |ui| self.tray_settings(ui));
                return;
            }
            _ => {}
        }
        if self.tab != Tab::Lighting || self.light.custom {
            theme::card(ui, |ui| {
                ui.add_space(8.0);
                ui.vertical_centered(|ui| self.keyboard(ui));
                ui.add_space(10.0);
                if let Some(t) = self.keyboard_legend() {
                    theme::help(ui, t);
                }
            });
        }
        match self.tab {
            Tab::Performance => self.performance_editor(ui),
            Tab::Keymap => theme::card(ui, |ui| self.remap_editor(ui)),
            Tab::Advanced => theme::card(ui, |ui| {
                self.pair_mode_picker(ui);
                ui.separator();
                self.pairs_editor(ui);
            }),
            Tab::Lighting => theme::card(ui, |ui| self.lighting_tab(ui)),
            Tab::Profiles | Tab::Settings => {}
        }
    }

    fn gen2_page(&mut self, ui: &mut egui::Ui) {
        let page = self.gen2.page();
        let blurb = self.gen2.blurb();
        if page == Tab::Lighting {
            // Presets sit beside the title so the keyboard doesn't move down.
            let mut changed = false;
            theme::page_header_with(ui, page.title(), blurb, |ui| changed = self.gen2.light_presets(ui, &mut self.store.light_presets2));
            if changed {
                self.persist();
            }
        } else {
            theme::page_header(ui, page.title(), blurb);
        }
        let mut names_changed = false;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            names_changed = self.gen2.body(ui, &mut self.store.macro_names);
            if page == Tab::Profiles {
                theme::card(ui, |ui| self.backup_card(ui));
            }
            if page == Tab::Settings {
                self.common_settings(ui);
            }
        });
        if names_changed {
            self.persist();
        }
    }
}

/// GPU renderer (eframe + OpenGL); the software renderer calls the same two methods.
impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        self.input_hook(raw_input);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.frame(ctx);
    }
}

impl App {
    /// The font atlas is as wide as the GPU allows (often 16384 px); 2048 is
    /// plenty for this UI and keeps the atlas small.
    pub fn input_hook(&mut self, raw_input: &mut egui::RawInput) {
        raw_input.max_texture_side = Some(raw_input.max_texture_side.map_or(2048, |m| m.min(2048)));
    }

    /// One UI frame.
    pub fn frame(&mut self, ctx: &egui::Context) {
        self.poll_device();
        self.gen2.poll();
        self.poll_backup();
        self.pick_board();
        self.log_memory(ctx);
        let gen2 = self.board == Board::Gen2;
        egui::TopBottomPanel::top("top").frame(theme::chrome_frame(egui::Margin::symmetric(16, 8))).show(ctx, |ui| self.header(ui));
        egui::TopBottomPanel::bottom("status").frame(theme::chrome_frame(egui::Margin::symmetric(16, 5))).show(ctx, |ui| self.status_bar(ui));
        egui::SidePanel::left("nav").resizable(false).exact_width(196.0).frame(theme::chrome_frame(egui::Margin::symmetric(10, 14))).show(ctx, |ui| {
            if gen2 {
                let mut page = self.gen2.page();
                let changed = {
                    let g = &self.gen2;
                    theme::nav(ui, &mut page, |p| g.page_dirty(p), |_| true)
                };
                if changed {
                    self.gen2.set_page(page);
                }
            } else {
                let mut page = self.tab;
                let changed = theme::nav(ui, &mut page, |p| self.page_dirty(p), |_| true);
                if changed {
                    self.set_tab(page);
                }
            }
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.label(egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).small().color(pal().muted));
                ui.horizontal(|ui| {
                    if let Some(t) = &self.icon {
                        ui.add(egui::Image::from_texture((t.id(), Vec2::splat(28.0))));
                    }
                    ui.label(theme::strong("Kraken Control"));
                });
            });
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(pal().bg).inner_margin(egui::Margin::symmetric(24, 18)))
            .show(ctx, |ui| if gen2 { self.gen2_page(ui) } else { self.gen1_page(ui) });
        if !gen2 {
            self.light_live(ctx);
        }
        self.sync_tracking(ctx);
    }
}

/// A back-up file that was read and checked, waiting for the user's choice.
struct PendingImport {
    path: std::path::PathBuf,
    file: backup::File,
    keyboard: bool,
    kraken: bool,
}

/// Export / import (Profiles page).
impl App {
    fn shown_keyboard(&self) -> Option<String> {
        match self.board {
            Board::Gen2 => self.gen2.export_backup().map(|g| g.board),
            Board::Gen1 => self.connected_model().map(|m| m.name().to_string()),
        }
    }

    fn backup_card(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Move to another PC");
        theme::help(
            ui,
            "Saves every page (performance, keymap, advanced keys, lighting, keyboard settings) and Kraken's own settings \
             (theme, drawing, lighting presets, saved profiles, per-app rules) to one file. Import it after a fresh install or on another PC.",
        );
        let busy = self.backup_job.is_some();
        ui.horizontal(|ui| {
            if ui.add_enabled(!busy, egui::Button::new("Export to file…")).clicked() {
                self.start_file_dialog(ui.ctx(), true);
            }
            if ui.add_enabled(!busy, egui::Button::new("Import from file…")).clicked() {
                self.start_file_dialog(ui.ctx(), false);
            }
            if busy {
                ui.spinner();
                theme::help(ui, "Choose a file in the window that opened.");
            }
        });
        theme::help(ui, "The export has what the pages show now, including changes not saved to the keyboard yet.");
        if self.board == Board::Gen2 {
            theme::help(ui, "Import also takes a profile exported by the official web driver (key travel, keymap and advanced keys).");
        }

        let shown = self.shown_keyboard();
        let gen2 = self.board == Board::Gen2;
        let Some(p) = self.backup_pending.as_mut() else { return };
        ui.separator();
        let name = p.path.file_name().map_or_else(|| p.path.display().to_string(), |n| n.to_string_lossy().into_owned());
        ui.label(theme::strong(format!("Import {name}")));
        let date = p.file.created.chars().take(10).collect::<String>();
        let official = p.file.keyboard.as_ref().is_some_and(backup::Keyboard::official);
        if official {
            theme::help(ui, format!("Exported by the official web driver on {date}."));
        } else {
            theme::help(ui, format!("Exported by Kraken Control {} on {date}.", p.file.app_version));
        }
        if let Some(k) = p.file.keyboard.as_ref().filter(|_| official) {
            // The official file names no keyboard: any connected gen-2 board may take it
            // (the import itself still checks that the layout matches).
            let fits = gen2 && shown.is_some();
            ui.add_enabled_ui(fits, |ui| {
                ui.checkbox(&mut p.keyboard, "Key travel, keymap and advanced keys from the official driver");
            });
            if fits {
                theme::help(ui, "Go into the current profile as unsaved changes: review the pages, then press Save to keyboard. Lighting, keyboard settings and macros are not in this file and stay as they are.");
            } else {
                p.keyboard = false;
                theme::note(ui, format!("Connect a 2025 keyboard to import this {}.", k.name()));
            }
        } else if let Some(k) = &p.file.keyboard {
            let fits = shown.as_deref() == Some(k.name().as_str());
            ui.add_enabled_ui(fits, |ui| {
                ui.checkbox(&mut p.keyboard, format!("Keyboard settings for the {}", k.name()));
            });
            if fits {
                theme::help(ui, "Go into the current profile as unsaved changes: review the pages, then press Save to keyboard. Gen 1 lighting changes right away.");
            } else {
                p.keyboard = false;
                theme::note(ui, match &shown {
                    Some(s) => format!("These are for the {}; the keyboard shown now is the {s}, so they can't be used here.", k.name()),
                    None => format!("Connect the {} to import its keyboard settings.", k.name()),
                });
            }
        }
        if let Some(k) = &p.file.kraken {
            ui.checkbox(&mut p.kraken, "Kraken settings");
            theme::help(ui, format!("{}. Presets and profiles with the same name are replaced.", k.summary()));
        }
        let any = p.keyboard || p.kraken;
        let mut go = false;
        let mut cancel = false;
        ui.horizontal(|ui| {
            go = theme::primary_button(ui, any, "Import").clicked();
            cancel = ui.button("Cancel").clicked();
        });
        if cancel {
            self.backup_pending = None;
        } else if go {
            self.apply_import(ui.ctx());
        }
    }

    fn start_file_dialog(&mut self, ctx: &egui::Context, export: bool) {
        let name = backup::suggested_name(self.shown_keyboard().as_deref());
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let path = sys::file_dialog(export, &name);
            let _ = tx.send((export, path));
            ctx.request_repaint();
        });
        self.backup_job = Some(rx);
    }

    fn poll_backup(&mut self) {
        let Some(rx) = &self.backup_job else { return };
        let (export, path) = match rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => (false, None),
        };
        self.backup_job = None;
        let Some(path) = path else { return };
        if export {
            self.export_to(&path);
        } else {
            match backup::read(&path) {
                Ok(file) => {
                    let (keyboard, kraken) = (file.keyboard.is_some(), file.kraken.is_some());
                    self.backup_pending = Some(PendingImport { path, file, keyboard, kraken });
                }
                Err(e) => self.set_status(e, true),
            }
        }
    }

    fn export_to(&mut self, path: &std::path::Path) {
        let keyboard = match self.board {
            Board::Gen2 => self.gen2.export_backup().map(|g| backup::Keyboard::Gen2(Box::new(g))),
            Board::Gen1 if self.connected_model().is_some() || self.demo => Some(backup::Keyboard::Gen1(Box::new(backup::Gen1 {
                model: self.model,
                settings: self.working_profile(String::new()),
                lighting: self.light.clone(),
                knob: Some(self.knob),
            }))),
            Board::Gen1 => None,
        };
        let kraken = backup::Kraken {
            theme: self.store.settings.theme,
            font: self.store.settings.font,
            renderer: self.store.settings.renderer,
            light_presets: self.store.light_presets.clone(),
            light_presets2: self.store.light_presets2.clone(),
            profiles: self.store.profiles.clone(),
            agent: self.agent_cfg.clone(),
            macro_names: self.store.macro_names.clone(),
        };
        let what = if keyboard.is_some() { "keyboard and Kraken settings" } else { "Kraken settings (no keyboard connected)" };
        let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        match backup::write(path, &backup::new_file(keyboard, Some(kraken))) {
            Ok(()) => {
                crate::log::line(format!("exported {what} to {}", path.display()));
                self.set_status(format!("Exported {what} to {name}"), false);
            }
            Err(e) => self.set_status(format!("Couldn't save {name}: {e}"), true),
        }
    }

    fn apply_import(&mut self, ctx: &egui::Context) {
        let Some(p) = self.backup_pending.take() else { return };
        let mut done = Vec::new();
        let mut notes: Vec<String> = Vec::new();
        if p.keyboard {
            match p.file.keyboard {
                Some(backup::Keyboard::Gen1(g)) if self.board == Board::Gen1 && g.model == self.model => {
                    let mut g = *g;
                    match backup::check_gen1_profile(&mut g.settings, &self.factory, &self.layout) {
                        Ok(n) => {
                            notes.extend(n);
                            let s = g.settings;
                            self.travel.ap.copy_from_slice(&s.ap);
                            self.travel.ds.copy_from_slice(&s.ds);
                            self.travel.us.copy_from_slice(&s.us);
                            self.flags = s.flags;
                            self.lw = s.lw;
                            self.rdt = s.rdt;
                            self.match_mode = s.match_mode;
                            self.lw_replace = s.lw_replace;
                            self.keymap_dirty |= self.keymap != s.keymap;
                            self.keymap = s.keymap;
                            if let Some(mut k) = g.knob {
                                if backup::check_knob(&mut k) > 0 {
                                    notes.push("knob actions not offered by Kraken were left out".into());
                                }
                                self.knob_dirty |= self.knob != k;
                                self.knob = k;
                            }
                            if backup::check_lighting1(&mut g.lighting) {
                                notes.push("lighting values adjusted".into());
                            }
                            if g.lighting.keys != self.light.keys {
                                self.push_light_undo(self.light.keys.clone());
                            }
                            // Lighting has no Save step on gen 1: it is sent now.
                            g.lighting.sent = self.light.sent;
                            self.light = g.lighting;
                            self.light_due = Some(Instant::now());
                            done.push("keyboard settings");
                        }
                        Err(e) => notes.push(e),
                    }
                }
                Some(backup::Keyboard::Gen2(g)) if self.board == Board::Gen2 => match self.gen2.import_backup(&g) {
                    Ok(n) => {
                        notes.extend(n);
                        done.push("keyboard settings");
                    }
                    Err(e) => notes.push(e),
                },
                Some(k) => notes.push(format!("keyboard settings are for the {}, skipped", k.name())),
                None => {}
            }
        }
        if p.kraken {
            if let Some(mut k) = p.file.kraken {
                notes.extend(backup::check_kraken(&mut k));
                self.merge_kraken(ctx, k);
                done.push("Kraken settings");
            }
        }
        crate::log::line(format!("imported {} from {} ({})", done.join(" + "), p.path.display(), notes.join("; ")));
        let mut msg = if done.is_empty() { "Nothing imported".to_string() } else { format!("Imported {}", done.join(" and ")) };
        if done.contains(&"keyboard settings") {
            msg.push_str(". Review the pages, then press Save to keyboard");
        }
        if !notes.is_empty() {
            msg.push_str(&format!(" ({})", notes.join("; ")));
        }
        self.set_status(msg, done.is_empty());
    }

    /// Kraken settings from a back-up: same-named presets / profiles / app rules are replaced.
    fn merge_kraken(&mut self, ctx: &egui::Context, k: backup::Kraken) {
        if k.theme != theme::current() || k.font != self.store.settings.font {
            theme::apply(ctx, k.theme, k.font);
        }
        self.store.settings.theme = k.theme;
        self.store.settings.font = k.font;
        self.store.settings.renderer = k.renderer;
        for p in k.light_presets {
            match self.store.light_presets.iter_mut().find(|q| q.model == p.model && q.name == p.name) {
                Some(q) => *q = p,
                None => self.store.light_presets.push(p),
            }
        }
        for m in k.macro_names {
            store::set_macro_name(&mut self.store.macro_names, &m.board, m.profile, m.id, &m.name);
        }
        for p in k.light_presets2 {
            match self.store.light_presets2.iter_mut().find(|q| q.board == p.board && q.name == p.name) {
                Some(q) => *q = p,
                None => self.store.light_presets2.push(p),
            }
        }
        for p in k.profiles {
            match self.store.profiles.iter_mut().find(|q| q.model == p.model && q.name == p.name) {
                Some(q) => *q = p,
                None => self.store.profiles.push(p),
            }
        }
        self.agent_cfg.auto_switch = k.agent.auto_switch;
        self.agent_cfg.default_slot = k.agent.default_slot;
        for r in k.agent.rules {
            match self.agent_cfg.rules.iter_mut().find(|q| q.exe == r.exe) {
                Some(q) => *q = r,
                None => self.agent_cfg.rules.push(r),
            }
        }
        self.persist();
        self.save_agent_cfg();
    }
}

/// Same visible lighting (per-key colours only matter for Custom).
fn same_look(a: &store::Lighting1, b: &store::Lighting1) -> bool {
    a.custom == b.custom
        && a.bright == b.bright
        && if a.custom { a.keys == b.keys } else { a.effect == b.effect && a.speed == b.speed && a.colour == b.colour }
}

/// Same RDT tables at wire resolution.
fn same_rdt(a: &[RdtPair], b: &[RdtPair]) -> bool {
    let units = |v: f32| (v * 200.0).round() as i32;
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.main == y.main && x.trigger == y.trigger && units(x.active_mm) == units(y.active_mm) && units(x.reset_mm) == units(y.reset_mm)
        })
}

fn key_rect(origin: Pos2, unit: f32, gap: f32, k: &KeyDef) -> Rect {
    let min = origin + Vec2::new(k.x * unit + gap / 2.0, k.y * unit + gap / 2.0);
    Rect::from_min_size(min, Vec2::new(k.w * unit - gap, k.h * unit - gap))
}

fn ap_color(ap: f32) -> Color32 {
    theme::travel_color(ap)
}

fn none_dash(s: String) -> String {
    if s.is_empty() {
        "nothing".into()
    } else {
        s
    }
}
