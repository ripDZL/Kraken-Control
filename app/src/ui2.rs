//! UI for gen-2 boards (VID 0x19F5). Self-contained: own device worker,
//! state and panels; `ui.rs` hosts it and switches between boards.

use std::collections::BTreeSet;
use std::time::{Duration, Instant};

use dd_proto::gen2::{self, func, Action, Advanced, Dks, LayoutKey, MacroEvent, MacroKey, PairKind, RtMode, Trigger};
use dd_proto::pattern;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::device2::{self, Cmd, Event, Snapshot, Table};
use crate::backup;
use crate::store::LightPreset2;
use crate::theme::{self, pal, Cap, Page, PresetAction};

/// Pages of the gen-2 UI (shared list, see `theme::Page`).
type Tab = Page;

/// Advanced key being created on the Advanced tab.
#[derive(Clone, Copy, PartialEq)]
enum Draft {
    /// SOCD / Rappy Snappy / One Key Switch; `priority` is used by SOCD only.
    Pair { kind: PairKind, keys: [Option<usize>; 2], priority: u8 },
    /// `slot` is a matrix slot; `entry` is set while editing an existing DKS key.
    Dks { slot: Option<usize>, entry: Option<usize>, dks: Dks },
    ModTap { key: Option<usize>, tap: Action, hold: Action, hold_ms: u16 },
    Toggle { key: Option<usize>, action: Action },
    /// Waiting for a click on the key that should play macro `id`.
    MacroBind { id: usize },
}

/// Macro being recorded / edited (Advanced keys > Macros).
struct MacroEdit {
    id: usize,
    events: Vec<MacroEvent>,
    recording: bool,
    /// Record the real pauses between keys (else 50 ms each, the official default).
    timed: bool,
    last_event: Option<Instant>,
    mods: egui::Modifiers,
}

/// Time between live colour reads (Lighting page). A read is 7 short requests.
const LED_POLL: Duration = Duration::from_millis(60);

/// Pause the official recorder puts between events unless real timing is kept.
const MACRO_DELAY_MS: u16 = 50;

const KNOB_NAMES: [&str; 3] = ["Knob press", "Knob left", "Knob right"];
/// DKS stages in stroke order (see `gen2::DksBind`).
const DKS_STAGES: [&str; 4] = ["Press", "Bottom", "Release", "Top"];

pub struct Gen2Panel {
    dev: device2::Handle,
    pub connected: Option<&'static str>,
    status: String,
    status_is_error: bool,
    busy: bool,

    snap: Option<Snapshot>,
    triggers: Vec<Trigger>,
    func: Vec<u8>,
    layout: &'static [LayoutKey],
    /// Layout key -> matrix slot.
    slots: Vec<Option<u8>>,

    /// Working keymap (4 layers) and per-key colours; `snap` holds the keyboard's copy.
    keymap: Vec<Vec<Action>>,
    colors: Vec<[u8; 3]>,
    /// Working advanced-key tables (MT/RS/SOCD/OKS, TGL, DKS).
    mt: Vec<u8>,
    tgl: Vec<u8>,
    dks: Vec<u8>,
    /// Macros by id (32 lists; empty Vec = the keyboard's macro block wasn't readable),
    /// the keyboard's copy, and what the last Save sent.
    macros: Vec<Vec<MacroEvent>>,
    macros_synced: Vec<Vec<MacroEvent>>,
    macros_sent: Option<Vec<Vec<MacroEvent>>>,
    macro_edit: Option<MacroEdit>,
    /// Live view (Lighting page, built-in effects): colours the keyboard shows now,
    /// when the last read was sent (None = none in flight) and whether the keyboard answers.
    leds: Vec<u8>,
    /// What was on screen when `leds` arrived, when that was, and the time between
    /// the last two reads: keys fade from one read to the next instead of jumping.
    leds_prev: Vec<u8>,
    leds_at: Instant,
    led_period: Duration,
    led_pending: Option<Instant>,
    led_next: Instant,
    led_off: bool,
    /// Knob press / left / right matrix slots.
    knob: [Option<u8>; 3],
    /// Knob action selected for remapping (index into `knob`).
    knob_sel: Option<usize>,
    draft: Option<Draft>,

    tab: Tab,
    selected: BTreeSet<usize>,
    layer: usize,
    /// Brush colour for "Custom" lighting.
    paint: [u8; 3],
    /// Second gradient colour and the axis for generated patterns.
    paint2: [u8; 3],
    pattern_dir: pattern::Direction,
    /// Index into `pattern::PRESETS`.
    palette: usize,
    /// Brightness to restore when the backlight is switched back on.
    last_brightness: u8,
    /// Drag painting (Custom lighting): last pointer position of the stroke.
    painting: Option<Pos2>,
    /// Per-key colours before each stroke / fill / undo step, newest last.
    color_undo: Vec<Vec<[u8; 3]>>,
    /// When pending lighting edits may be written: now for clicks and painting,
    /// a short pause later while a slider / colour picker is still held.
    light_due: Option<Instant>,
    /// Name typed for a new lighting preset.
    preset_name: String,
    demo: bool,
}

/// Settings fields the UI edits (saved as read-modify-write edits).
const EDITABLE: &[func::Field] = &[
    func::POLLING,
    func::TICK,
    func::DEBOUNCE,
    func::ADAPTIVE,
    func::LOCK_WIN,
    func::LOCK_ALT_TAB,
    func::LOCK_ALT_F4,
    func::LIGHT_EFFECT,
    func::LIGHT_BRIGHTNESS,
    func::LIGHT_SPEED_INV,
    func::LIGHT_MULTI,
    func::LIGHT_R,
    func::LIGHT_G,
    func::LIGHT_B,
    func::LIGHT_DIRECTION,
];

/// Keyboard settings and lighting fields saved in back-up files, by name.
const BACKUP_FIELDS: &[(&str, func::Field)] = &[
    ("polling", func::POLLING),
    ("tick", func::TICK),
    ("debounce", func::DEBOUNCE),
    ("adaptive_stable", func::ADAPTIVE),
    ("lock_win", func::LOCK_WIN),
    ("lock_alt_tab", func::LOCK_ALT_TAB),
    ("lock_alt_f4", func::LOCK_ALT_F4),
    ("light_effect", func::LIGHT_EFFECT),
    ("light_brightness", func::LIGHT_BRIGHTNESS),
    ("light_speed_inv", func::LIGHT_SPEED_INV),
    ("light_multi", func::LIGHT_MULTI),
    ("light_r", func::LIGHT_R),
    ("light_g", func::LIGHT_G),
    ("light_b", func::LIGHT_B),
    ("light_direction", func::LIGHT_DIRECTION),
];

/// Values the Settings and Lighting pages can produce for `field`.
fn backup_value_ok(field: func::Field, v: u8) -> bool {
    let in_list = |opts: &[(u8, &str)]| opts.iter().any(|o| o.0 == v);
    match field {
        f if f == func::POLLING => in_list(func::POLLING_RATES),
        f if f == func::TICK => in_list(func::TICK_RATES),
        f if f == func::DEBOUNCE => in_list(func::DEBOUNCE_LEVELS),
        f if f == func::LIGHT_EFFECT => (v as usize) < gen2::EFFECTS.len(),
        f if f == func::LIGHT_BRIGHTNESS => v <= 100,
        f if f == func::LIGHT_SPEED_INV => v <= 4,
        f if f == func::LIGHT_R || f == func::LIGHT_G || f == func::LIGHT_B => true,
        // Switches: adaptive, locks, multi-colour.
        _ => v <= 1,
    }
}

/// Lighting fields the Lighting tab writes live (no Save needed).
const LIGHTING: &[func::Field] = &[
    func::LIGHT_EFFECT,
    func::LIGHT_BRIGHTNESS,
    func::LIGHT_SPEED_INV,
    func::LIGHT_MULTI,
    func::LIGHT_R,
    func::LIGHT_G,
    func::LIGHT_B,
    // Added after presets existed: keep it last (older presets have one field less).
    func::LIGHT_DIRECTION,
];

impl Gen2Panel {
    pub fn new(ctx: egui::Context, demo: bool) -> Self {
        let dev = device2::spawn(move || ctx.request_repaint());
        let mut p = Self {
            dev,
            connected: None,
            status: "Looking for gen-2 keyboard…".into(),
            status_is_error: false,
            busy: false,
            snap: None,
            triggers: Vec::new(),
            func: vec![0; gen2::FUNC_LEN],
            layout: gen2::layout_k75(),
            slots: Vec::new(),
            keymap: Vec::new(),
            colors: Vec::new(),
            mt: Vec::new(),
            tgl: Vec::new(),
            dks: Vec::new(),
            macros: Vec::new(),
            macros_synced: Vec::new(),
            macros_sent: None,
            macro_edit: None,
            leds: Vec::new(),
            leds_prev: Vec::new(),
            leds_at: Instant::now(),
            led_period: LED_POLL,
            led_pending: None,
            led_next: Instant::now(),
            led_off: false,
            knob: [None; 3],
            knob_sel: None,
            draft: None,
            tab: Tab::Performance,
            selected: BTreeSet::new(),
            layer: 0,
            paint: [255, 60, 30],
            paint2: [0, 60, 255],
            pattern_dir: pattern::Direction::LeftToRight,
            palette: 0,
            last_brightness: 100,
            painting: None,
            color_undo: Vec::new(),
            light_due: None,
            preset_name: String::new(),
            demo,
        };
        if demo {
            p.enter_demo();
            if let Some(page) = std::env::args().find_map(|a| a.strip_prefix("--g2tab=").and_then(Page::from_arg)) {
                p.tab = page;
            }
        }
        p
    }

    fn enter_demo(&mut self) {
        let layout = gen2::layout_k75();
        let mut factory: Vec<Action> = layout
            .iter()
            .map(|k| match k.code {
                255 => Action { kind: gen2::kind::FUNCTION, code1: 255, code2: 1 },
                254 => Action { kind: gen2::kind::FUNCTION, code1: 255, code2: 2 },
                c @ 224..=231 => Action { kind: gen2::kind::KEY, code1: 1 << (c - 224), code2: 0 },
                c => Action { kind: gen2::kind::KEY, code1: 0, code2: c as u8 },
            })
            .chain(std::iter::repeat(Action::default()))
            .take(gen2::KEYS)
            .collect();
        for (i, c) in [(92usize, 205u8), (93, 233), (94, 234)] {
            factory[i] = Action { kind: gen2::kind::MEDIA, code1: c, code2: 0 };
        }
        let mut user0 = factory.clone();
        let (w, s_, caps) = (44usize, 46usize, 58usize); // demo: W/S SOCD, Caps mod-tap (layout order = slot order)
        let (aw, as_) = (factory[w], factory[s_]);
        user0[w] = Action { kind: gen2::kind::SOCD, code1: 0, code2: s_ as u8 };
        user0[s_] = Action { kind: gen2::kind::SOCD, code1: 1, code2: w as u8 };
        user0[caps] = Action { kind: gen2::kind::MT, code1: 2, code2: 20 };
        let mut mt = vec![0u8; gen2::MT_LEN];
        gen2::set_mt_entry(&mut mt, 0, aw, as_);
        gen2::set_mt_entry(&mut mt, 1, as_, aw);
        gen2::set_mt_entry(&mut mt, 2, Action { kind: gen2::kind::KEY, code1: 0, code2: 41 }, Action { kind: gen2::kind::KEY, code1: 1, code2: 0 });
        // demo: Q = DKS (Q on the way down, E held from the bottom to the top)
        let at = |name: &str| layout.iter().position(|k| k.name == name).unwrap_or(0);
        let (q, e) = (at("Q"), at("E"));
        let mut dks = vec![0u8; gen2::DKS_LEN];
        let mut demo_dks = Dks::default();
        demo_dks.binds[0] = gen2::DksBind { action: factory[q], spans: [2, 0, 0, 0] };
        demo_dks.binds[1] = gen2::DksBind { action: factory[e], spans: [0, 3, 0, 0] };
        gen2::set_dks_entry(&mut dks, 0, &demo_dks);
        user0[q] = Action { kind: gen2::kind::DKS, code1: 0, code2: 0 };
        let mut t = Trigger([0xA0, 1, 149, 0, 29, 0, 29, 0]);
        let mut triggers = vec![t; gen2::KEYS];
        for (i, tr) in triggers.iter_mut().enumerate().take(82) {
            tr.set_actuation_mm([1.5, 0.8, 0.3, 2.0, 3.5][i % 5]);
        }
        t.set_actuation_mm(0.2);
        t.set_mode(RtMode::Continuous);
        for i in [30, 44, 45, 46] {
            triggers[i] = t;
        }
        let mut func = vec![0u8; gen2::FUNC_LEN];
        func::set(&mut func, func::POLLING, 2);
        func::set(&mut func, func::LIGHT_EFFECT, if std::env::args().any(|a| a == "--g2custom") { 0 } else { 6 });
        func::set(&mut func, func::LIGHT_BRIGHTNESS, 80);
        func::set(&mut func, func::LIGHT_SPEED_INV, 2);
        func[14..17].copy_from_slice(&[255, 120, 40]);
        let snap = Snapshot {
            name: "A75 Ultra (2025)",
            fw: "121".into(),
            profile: 0,
            func,
            triggers,
            keymap: vec![user0, vec![Action::default(); gen2::KEYS], vec![Action::default(); gen2::KEYS], vec![Action::default(); gen2::KEYS]],
            factory: vec![factory.clone(), vec![Action::default(); gen2::KEYS], vec![Action::default(); gen2::KEYS], vec![Action::default(); gen2::KEYS]],
            colors: (0..gen2::KEYS).map(|i| [(i * 37 % 256) as u8, (120 + i * 13 % 120) as u8, 255 - (i * 23 % 200) as u8]).collect(),
            mt,
            tgl: vec![0; gen2::TGL_LEN],
            dks,
            macros: {
                // demo: M1 = Ctrl+C
                let ev = |code: u8, press: bool| MacroEvent { key: MacroKey::Key(code), press, delay_ms: MACRO_DELAY_MS };
                let mut m = gen2::encode_macros(&[vec![ev(224, true), ev(6, true), ev(6, false), ev(224, false)]]).unwrap_or_default();
                m.resize(gen2::MACRO_LEN, 0);
                m
            },
        };
        self.connected = Some(snap.name);
        self.apply(snap);
        // `--g2draft=dks|rs|oks` opens an editor on the Advanced keys page (screenshots).
        self.draft = match std::env::args().find_map(|a| a.strip_prefix("--g2draft=").map(str::to_string)).as_deref() {
            Some("dks") => Some(Draft::Dks { slot: Some(q), entry: Some(0), dks: demo_dks }),
            Some("rs") => Some(Draft::Pair { kind: PairKind::Rs, keys: [Some(q), Some(e)], priority: 0 }),
            Some("oks") => Some(Draft::Pair { kind: PairKind::Oks, keys: [Some(q), None], priority: 0 }),
            Some("macro") => {
                self.macro_edit = Some(MacroEdit { id: 0, events: self.macros[0].clone(), recording: false, timed: false, last_event: None, mods: egui::Modifiers::NONE });
                None
            }
            _ => None,
        };
        self.selected = [30usize, 44, 45, 46].into_iter().collect();
        if std::env::args().any(|a| a == "--g2tab=keymap") {
            self.selected = [30usize].into_iter().collect();
        }
        self.set_status("Demo mode — no keyboard I/O", false);
    }

    fn set_status(&mut self, s: impl Into<String>, err: bool) {
        self.status = s.into();
        self.status_is_error = err;
    }

    fn apply(&mut self, s: Snapshot) {
        self.slots = gen2::match_layout(self.layout, &s.factory[0]);
        let missing = self.slots.iter().filter(|s| s.is_none()).count();
        if missing > 0 {
            crate::log::line(format!("gen2: {missing} layout keys not found in factory keymap"));
        }
        self.triggers = s.triggers.clone();
        self.func = s.func.clone();
        self.keymap = s.keymap.clone();
        self.colors = s.colors.clone();
        self.mt = s.mt.clone();
        self.tgl = s.tgl.clone();
        self.dks = s.dks.clone();
        self.macros_synced = if s.macros.len() == gen2::MACRO_LEN { gen2::parse_macros(&s.macros, &gen2::macros_bound(&s.keymap)) } else { Vec::new() };
        self.macros = self.macros_synced.clone();
        self.macros_sent = None;
        self.macro_edit = None;
        self.leds.clear();
        self.leds_prev.clear();
        self.led_pending = None;
        self.led_off = false;
        self.knob = gen2::knob_slots(&s.factory[0]);
        self.draft = None;
        self.painting = None;
        self.color_undo.clear();
        self.light_due = None;
        let b = func::get(&s.func, func::LIGHT_BRIGHTNESS);
        if b > 0 {
            self.last_brightness = b;
        }
        self.snap = Some(s);
    }

    /// True when a device event arrived (caller may want to switch view).
    pub fn poll(&mut self) -> bool {
        let mut any = false;
        while let Ok(ev) = self.dev.rx.try_recv() {
            any = true;
            match ev {
                Event::Connected { name, pid } => {
                    self.connected = Some(name);
                    self.set_status(format!("Connected: {name} (PID {pid:04X})"), false);
                }
                Event::Unsupported(msg) => self.set_status(msg, true),
                Event::Disconnected(why) => {
                    self.connected = None;
                    self.snap = None;
                    self.busy = false;
                    self.painting = None;
                    self.light_due = None;
                    self.set_status(format!("Disconnected ({why})"), true);
                }
                Event::Snapshot(s) => self.apply(*s),
                Event::Triggers(t) => {
                    self.triggers = t.clone();
                    if let Some(s) = self.snap.as_mut() {
                        s.triggers = t;
                    }
                }
                Event::Func(f) => match self.snap.as_mut() {
                    Some(s) => {
                        merge_pending(&mut self.func, &s.func, &f);
                        s.func = f;
                    }
                    None => self.func = f,
                },
                Event::Keymap { layer, actions } => {
                    if let Some(l) = self.keymap.get_mut(layer) {
                        *l = actions.clone();
                    }
                    if let Some(l) = self.snap.as_mut().and_then(|s| s.keymap.get_mut(layer)) {
                        *l = actions;
                    }
                }
                Event::Colors(c) => match self.snap.as_mut() {
                    Some(s) => {
                        merge_pending(&mut self.colors, &s.colors, &c);
                        s.colors = c;
                    }
                    None => self.colors = c,
                },
                Event::Table { table, data } => {
                    let (work, synced) = match table {
                        Table::Mt => (&mut self.mt, self.snap.as_mut().map(|s| &mut s.mt)),
                        Table::Tgl => (&mut self.tgl, self.snap.as_mut().map(|s| &mut s.tgl)),
                        Table::Dks => (&mut self.dks, self.snap.as_mut().map(|s| &mut s.dks)),
                    };
                    *work = data.clone();
                    if let Some(s) = synced {
                        *s = data;
                    }
                }
                Event::Macros(back) => {
                    // What was sent is known exactly; a re-parse can't tell an empty
                    // macro from the first stored one (both have offset 64).
                    let sent = self.macros_sent.take().filter(|m| gen2::encode_macros(m).is_some_and(|d| back.get(..d.len()) == Some(&d[..])));
                    match sent {
                        Some(m) => self.macros_synced = m,
                        None => {
                            self.macros_synced = gen2::parse_macros(&back, &gen2::macros_bound(&self.keymap));
                            self.macros = self.macros_synced.clone();
                        }
                    }
                    if let Some(s) = self.snap.as_mut() {
                        s.macros = back;
                    }
                }
                Event::Leds(d) => {
                    self.led_pending = None;
                    self.led_off = d.is_empty();
                    // Start the next fade from what is on screen now.
                    let now = Instant::now();
                    self.leds_prev = (0..self.leds.len() / 3).flat_map(|s| self.led_rgb(s, now).unwrap_or_default()).collect();
                    self.led_period = now.duration_since(self.leds_at).clamp(Duration::from_millis(30), Duration::from_millis(400));
                    self.leds_at = now;
                    self.leds = d;
                }
                Event::Busy(b) => self.busy = b,
                Event::Done(m) => self.set_status(m, false),
                Event::Error(m) => self.set_status(m, true),
            }
        }
        any
    }

    fn triggers_dirty(&self) -> bool {
        self.snap.as_ref().is_some_and(|s| s.triggers != self.triggers)
    }

    fn func_edits(&self) -> Vec<(func::Field, u8)> {
        let Some(s) = &self.snap else { return Vec::new() };
        EDITABLE.iter().filter(|f| func::get(&s.func, **f) != func::get(&self.func, **f)).map(|f| (*f, func::get(&self.func, *f))).collect()
    }

    fn keymap_dirty_layers(&self) -> Vec<usize> {
        let Some(s) = &self.snap else { return Vec::new() };
        (0..self.keymap.len()).filter(|&l| s.keymap.get(l) != self.keymap.get(l)).collect()
    }

    fn colors_dirty(&self) -> bool {
        self.snap.as_ref().is_some_and(|s| s.colors != self.colors)
    }

    fn tables_dirty(&self) -> Vec<Table> {
        let Some(s) = &self.snap else { return Vec::new() };
        let mut v = Vec::new();
        if s.mt != self.mt {
            v.push(Table::Mt);
        }
        if s.tgl != self.tgl {
            v.push(Table::Tgl);
        }
        if s.dks != self.dks {
            v.push(Table::Dks);
        }
        v
    }

    /// Lighting page with a built-in effect on a real keyboard: the on-screen
    /// keyboard shows what the keys are doing (the official page does the same).
    fn live_view(&self) -> bool {
        self.tab == Tab::Lighting && !self.led_off && self.snap.is_some() && func::get(&self.func, func::LIGHT_EFFECT) != 0
    }

    /// Live colour of matrix slot `s`: the last read, faded in from the one before.
    fn led_rgb(&self, s: usize, now: Instant) -> Option<[u8; 3]> {
        let new = self.leds.get(3 * s..3 * s + 3)?;
        let Some(old) = self.leds_prev.get(3 * s..3 * s + 3) else { return Some([new[0], new[1], new[2]]) };
        let t = (now.duration_since(self.leds_at).as_secs_f32() / self.led_period.as_secs_f32()).clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
        Some([mix(old[0], new[0]), mix(old[1], new[1]), mix(old[2], new[2])])
    }

    /// Asks for the live colours again `LED_POLL` after the last request, one read
    /// at a time (the official page waits 300 ms, which looks choppy).
    fn led_poll(&mut self, ctx: &egui::Context) {
        if self.demo {
            // No keyboard: a fixed rainbow by matrix slot stands in for the live colours.
            self.leds = (0..gen2::KEYS).flat_map(|s| egui::ecolor::Hsva::new(s as f32 / 95.0 % 1.0, 0.85, 1.0, 1.0).to_srgb()).collect();
            return;
        }
        let now = Instant::now();
        if self.led_pending.is_some_and(|t| now.duration_since(t) > Duration::from_secs(3)) {
            self.led_pending = None;
        }
        if self.led_pending.is_none() && !self.busy && now >= self.led_next {
            self.dev.send(Cmd::ReadLeds);
            self.led_pending = Some(now);
            self.led_next = now + LED_POLL;
        }
        // ~30 frames a second while the fade runs.
        ctx.request_repaint_after(Duration::from_millis(33));
    }

    fn macros_dirty(&self) -> bool {
        self.macros != self.macros_synced
    }

    fn dirty(&self) -> bool {
        self.macros_dirty()
            || self.triggers_dirty()
            || !self.func_edits().is_empty()
            || !self.keymap_dirty_layers().is_empty()
            || self.colors_dirty()
            || !self.tables_dirty().is_empty()
    }

    fn save(&mut self) {
        let Some(profile) = self.snap.as_ref().map(|s| s.profile) else { return };
        if self.triggers_dirty() {
            self.dev.send(Cmd::SaveTriggers { profile, triggers: self.triggers.clone() });
        }
        let edits = self.func_edits();
        if !edits.is_empty() {
            self.dev.send(Cmd::SaveFunc { profile, edits });
        }
        // Tables before keymaps, so keys never point at entries not written yet.
        for table in self.tables_dirty() {
            let data = match table {
                Table::Mt => self.mt.clone(),
                Table::Tgl => self.tgl.clone(),
                Table::Dks => self.dks.clone(),
            };
            self.dev.send(Cmd::SaveTable { profile, table, data });
        }
        if self.macros_dirty() {
            if let Some(data) = gen2::encode_macros(&self.macros) {
                self.macros_sent = Some(self.macros.clone());
                self.dev.send(Cmd::SaveMacros { profile, data });
            }
        }
        for layer in self.keymap_dirty_layers() {
            self.dev.send(Cmd::SaveKeymap { profile, layer, actions: self.keymap[layer].clone() });
        }
        if self.colors_dirty() {
            self.dev.send(Cmd::SaveColors { profile, colors: self.colors.clone() });
        }
    }

    /// Writes pending lighting edits (effect fields, per-key colours) without
    /// Save, like the official driver: painting and clicks go out at once (also
    /// mid-stroke), a held slider / picker after a short pause; one write in
    /// flight, edits made meanwhile follow right after. Other edits wait for Save.
    fn live_lighting(&mut self, ctx: &egui::Context) {
        let Some(due) = self.light_due else { return };
        let now = Instant::now();
        let held = ctx.input(|i| i.pointer.primary_down());
        if self.busy || (held && self.painting.is_none() && now < due) {
            ctx.request_repaint_after(due.saturating_duration_since(now).max(Duration::from_millis(30)));
            return;
        }
        self.light_due = None;
        let Some(s) = self.snap.as_mut() else { return };
        if self.demo {
            for f in LIGHTING {
                func::set(&mut s.func, *f, func::get(&self.func, *f));
            }
            s.colors = self.colors.clone();
            return;
        }
        let profile = s.profile;
        let edits: Vec<_> = self.func_edits().into_iter().filter(|(f, _)| LIGHTING.contains(f)).collect();
        if !edits.is_empty() {
            self.dev.send(Cmd::SaveFunc { profile, edits });
            self.busy = true;
        }
        if self.colors_dirty() {
            self.dev.send(Cmd::SaveColors { profile, colors: self.colors.clone() });
            self.busy = true;
        }
    }

    // ------------------------------------------------------------ panels

    /// Reload / Save and the unsaved state (header, left).
    pub fn actions(&mut self, ui: &mut egui::Ui) {
        let p = pal();
        let ready = self.snap.is_some() && !self.busy && !self.demo;
        let dirty = self.dirty();
        if ui.add_enabled(ready, egui::Button::new("Reload from keyboard")).on_hover_text("Reads the keyboard again and replaces unsaved changes.").clicked() {
            self.dev.send(Cmd::Refresh);
        }
        if theme::primary_button(ui, ready && dirty, "Save to keyboard").clicked() {
            self.save();
        }
        if self.busy {
            ui.spinner();
        }
        if dirty {
            theme::dot(ui, p.unsaved);
            ui.label(egui::RichText::new("Unsaved changes").color(p.unsaved));
        }
    }

    /// Keyboard pill and profile (header, right-to-left). Returns the pill.
    pub fn device_info(&self, ui: &mut egui::Ui, switch: bool, compact: bool) -> egui::Response {
        let p = pal();
        let profile = self.snap.as_ref().map(|s| {
            let i = s.profile as usize;
            format!("Profile {}: {}", i + 1, gen2::PROFILE_NAMES.get(i).copied().unwrap_or("?"))
        });
        if let Some(t) = profile.as_ref().filter(|_| !compact) {
            ui.label(egui::RichText::new(t).color(p.muted));
        }
        let (dot, name, state) = match (&self.snap, self.connected) {
            (Some(s), _) => (p.accent, s.name.to_string(), "Connected"),
            (None, Some(n)) => (p.unsaved, n.to_string(), "Reading…"),
            (None, None) => (p.disabled, "No keyboard found".to_string(), ""),
        };
        let mut tip: Vec<String> = Vec::new();
        if let Some(s) = &self.snap {
            tip.push(format!("Firmware {}", s.fw));
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

    /// Last status message and whether it is an error.
    pub fn status(&self) -> (&str, bool) {
        (&self.status, self.status_is_error)
    }

    pub fn page(&self) -> Page {
        self.tab
    }

    pub fn set_page(&mut self, page: Page) {
        if page == Tab::Keymap && self.tab != Tab::Keymap && self.selected.len() > 1 {
            let first = *self.selected.iter().next().unwrap();
            self.selected = [first].into_iter().collect();
        }
        if page != self.tab {
            self.knob_sel = None;
        }
        self.tab = page;
    }

    /// Amber dot in the page menu (lighting is written live, never pending).
    pub fn page_dirty(&self, page: Page) -> bool {
        match page {
            Tab::Performance => self.triggers_dirty(),
            Tab::Keymap => !self.keymap_dirty_layers().is_empty(),
            Tab::Advanced => !self.tables_dirty().is_empty() || self.macros_dirty(),
            Tab::Settings => !self.func_edits().is_empty(),
            _ => false,
        }
    }

    pub fn blurb(&self) -> &'static str {
        match self.tab {
            Tab::Performance => "Choose how far each key travels before it registers, how quickly it resets, and its dead zones.",
            Tab::Keymap => "Change what each key and the knob send. Click a key, then pick its new action.",
            Tab::Advanced => "Give keys extra jobs: SOCD, Rappy Snappy and One Key Switch pairs, Mod Tap, Toggle, DKS and macros.",
            Tab::Lighting => "Changes show on the keyboard straight away. No need to save.",
            Tab::Profiles => "Switch between the four profiles stored on the keyboard.",
            Tab::Settings => "Keyboard options, the look of the app and troubleshooting.",
        }
    }

    fn profiles(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Profiles on the keyboard");
        let cur = self.snap.as_ref().map(|s| s.profile);
        let can = cur.is_some() && !self.busy && !self.dirty() && !self.demo;
        for i in 0..gen2::PROFILES {
            let b = egui::Button::new(format!("{}  {}", i + 1, gen2::PROFILE_NAMES[i as usize])).selected(cur == Some(i)).min_size(Vec2::new(150.0, 0.0));
            if ui.add_enabled(can, b).clicked() && cur != Some(i) {
                self.dev.send(Cmd::SelectProfile(i));
            }
        }
        theme::help(ui, if self.dirty() { "Save or reload first to switch profiles." } else { "Each profile has its own keys, travel and lighting." });
    }

    /// Content of the current page; the host draws header, menu and the shared Settings cards.
    pub fn body(&mut self, ui: &mut egui::Ui) {
        if self.snap.is_none() {
            if self.connected.is_some() {
                theme::help(ui, "Reading keyboard…");
            } else if self.tab != Tab::Settings {
                theme::no_keyboard(ui);
            }
            return;
        }
        let custom_light = func::get(&self.func, func::LIGHT_EFFECT) == 0;
        let live_view = self.live_view();
        if live_view {
            self.led_poll(ui.ctx());
        }
        if matches!(self.tab, Tab::Performance | Tab::Keymap | Tab::Advanced) || (self.tab == Tab::Lighting && custom_light) {
            theme::card(ui, |ui| {
                ui.add_space(8.0);
                ui.vertical_centered(|ui| self.keyboard(ui));
                ui.add_space(10.0);
            });
        }
        match self.tab {
            Tab::Performance => theme::card(ui, |ui| self.performance(ui)),
            Tab::Keymap => theme::card(ui, |ui| self.keymap_editor(ui)),
            Tab::Advanced => {
                theme::card(ui, |ui| self.advanced(ui));
                theme::card(ui, |ui| self.macros_card(ui));
            }
            Tab::Lighting => {
                theme::card(ui, |ui| self.lighting(ui));
                if live_view {
                    // Below the controls, so they stay in view without scrolling.
                    theme::card(ui, |ui| {
                        theme::section(ui, "Live view");
                        theme::help(ui, "The lights as the keyboard shows them right now.");
                        ui.add_space(4.0);
                        ui.vertical_centered(|ui| self.keyboard(ui));
                        ui.add_space(10.0);
                    });
                }
            }
            Tab::Profiles => theme::card(ui, |ui| self.profiles(ui)),
            Tab::Settings => theme::card(ui, |ui| {
                theme::section(ui, "Keyboard");
                self.settings(ui);
            }),
        }
        self.live_lighting(ui.ctx());
    }

    fn light_fields(&self) -> Vec<u8> {
        LIGHTING.iter().map(|f| func::get(&self.func, *f)).collect()
    }

    /// Lighting presets (saved on this PC): one click applies a look live.
    /// Shown beside the Lighting page title. Returns true when the preset list changed.
    pub fn light_presets(&mut self, ui: &mut egui::Ui, presets: &mut Vec<LightPreset2>) -> bool {
        let Some(board) = self.snap.as_ref().map(|s| s.name) else { return false };
        let idx: Vec<usize> = presets.iter().enumerate().filter(|(_, p)| p.board == board).map(|(i, _)| i).collect();
        let names: Vec<String> = idx.iter().map(|&i| presets[i].name.clone()).collect();
        let fields = self.light_fields();
        let custom = func::get(&self.func, func::LIGHT_EFFECT) == 0;
        let active = idx.iter().position(|&i| presets[i].fields == fields && (!custom || presets[i].colors == self.colors));
        let Some(action) = theme::presets_row(ui, &names, active, &mut self.preset_name) else { return false };
        match action {
            PresetAction::Apply(n) => {
                let p = presets[idx[n]].clone();
                self.apply_light_preset(&p);
                self.set_status(format!("Lighting preset '{}' applied", p.name), false);
                return false;
            }
            PresetAction::Save(name) => {
                let p = LightPreset2 { name: name.clone(), board: board.to_string(), fields, colors: self.colors.clone() };
                match presets.iter_mut().find(|q| q.board == board && q.name == name) {
                    Some(q) => *q = p,
                    None => presets.push(p),
                }
                self.set_status(format!("Lighting preset '{name}' saved"), false);
            }
            PresetAction::Replace(n) => {
                let p = &mut presets[idx[n]];
                p.fields = fields;
                p.colors = self.colors.clone();
                self.set_status(format!("Lighting preset '{}' replaced with the current lighting", names[n]), false);
            }
            PresetAction::Delete(n) => {
                presets.remove(idx[n]);
                self.set_status(format!("Lighting preset '{}' deleted", names[n]), false);
            }
        }
        true
    }

    /// Everything on the keyboard pages, as shown now (unsaved edits included).
    pub fn export_backup(&self) -> Option<backup::Gen2> {
        let s = self.snap.as_ref()?;
        Some(backup::Gen2 {
            board: s.name.to_string(),
            keys: self
                .triggers
                .iter()
                .map(|t| backup::Gen2Key {
                    actuation: t.actuation_mm(),
                    mode: t.mode() as u8,
                    press: t.press_mm(),
                    release: t.release_mm(),
                    top_dz: t.top_dz_mm(),
                    bottom_dz: t.bottom_dz_mm(),
                    priority: t.priority(),
                })
                .collect(),
            keymap: self.keymap.iter().map(|l| l.iter().map(|a| [a.kind, a.code1, a.code2]).collect()).collect(),
            settings: BACKUP_FIELDS.iter().map(|(n, f)| (n.to_string(), func::get(&self.func, *f))).collect(),
            colors: self.colors.clone(),
            mt: self.mt.clone(),
            tgl: self.tgl.clone(),
            dks: self.dks.clone(),
            macros: gen2::encode_macros(&self.macros).unwrap_or_default(),
        })
    }

    /// Puts a back-up's values in place as unsaved edits (Save to keyboard writes them).
    /// Values are clamped to the page ranges; key actions Kraken doesn't offer go back
    /// to factory. Returns notes about what was changed.
    pub fn import_backup(&mut self, g: &backup::Gen2) -> Result<Vec<String>, String> {
        let Some(s) = self.snap.as_ref() else { return Err("Connect the keyboard first.".into()) };
        // A file from the official driver doesn't name its keyboard; the layout check below still applies.
        if g.board != s.name && g.board != backup::OFFICIAL_BOARD {
            return Err(format!("The keyboard settings are for {}, not the {}.", g.board, s.name));
        }
        if g.keys.len() != self.triggers.len() || g.keymap.len() != self.keymap.len() || g.keymap.iter().zip(&self.keymap).any(|(a, b)| a.len() != b.len()) {
            return Err("The keyboard settings in the file don't match this keyboard's layout.".into());
        }
        let factory = s.factory.clone();
        let current = s.keymap.clone();
        let mut notes = Vec::new();
        let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });

        // Travel: only the values the Performance page edits; switch type and precision stay.
        for (t, k) in self.triggers.iter_mut().zip(&g.keys) {
            t.set_mode(match k.mode {
                1 => RtMode::On,
                2 => RtMode::Continuous,
                _ => RtMode::Off,
            });
            let fit = |v: f32, (lo, hi): (f32, f32)| v.is_finite().then(|| v.clamp(lo, hi.max(lo)));
            if let Some(v) = fit(k.actuation, (gen2::ACTUATION_MM.0, gen2::max_actuation_mm(t).min(gen2::ACTUATION_MM.1))) {
                t.set_actuation_mm(v);
            }
            if let Some(v) = fit(k.press, gen2::RT_MM) {
                t.set_press_mm(v);
            }
            if let Some(v) = fit(k.release, gen2::RT_MM) {
                t.set_release_mm(v);
            }
            if let Some(v) = fit(k.top_dz, gen2::DEADZONE_MM) {
                t.set_top_dz_mm(v);
            }
            if let Some(v) = fit(k.bottom_dz, gen2::DEADZONE_MM) {
                t.set_bottom_dz_mm(v);
            }
            t.set_priority(k.priority.min(2));
        }

        // Keymap: picker actions, factory actions, and advanced keys whose table entries hold such actions.
        let picker: Vec<Action> = gen2::picker().iter().flat_map(|(_, items)| items.iter().map(|i| i.action)).collect();
        let safe = |a: &Action| *a == Action::default() || picker.contains(a) || factory.iter().any(|l| l.contains(a));
        let mt_ok = g.mt.len() == gen2::MT_LEN;
        let tgl_ok = g.tgl.len() == gen2::TGL_LEN;
        // Back-ups from before v0.13 carry no DKS table: those keys stay as they are.
        let dks_ok = g.dks.len() == gen2::DKS_LEN;
        // Macros: only when the file has them and this keyboard's macro block is readable.
        let macros_in = (g.macros.len() >= gen2::MACRO_HEADER && g.macros.len() <= gen2::MACRO_LEN && !self.macros.is_empty()).then(|| {
            let layers: Vec<Vec<Action>> = g.keymap.iter().map(|l| l.iter().map(|v| Action { kind: v[0], code1: v[1], code2: v[2] }).collect()).collect();
            let mut block = g.macros.clone();
            block.resize(gen2::MACRO_LEN, 0);
            gen2::parse_macros(&block, &gen2::macros_bound(&layers))
        });
        let (mut reset, mut dks) = (0, 0);
        for (l, layer) in g.keymap.iter().enumerate() {
            for (i, v) in layer.iter().enumerate() {
                let a = Action { kind: v[0], code1: v[1], code2: v[2] };
                let cur = current.get(l).and_then(|x| x.get(i)).copied().unwrap_or_default();
                let fac = factory.get(l).and_then(|x| x.get(i)).copied().unwrap_or_default();
                let e = a.code1 as usize;
                let ok = match a.kind {
                    gen2::kind::DKS if dks_ok => e < gen2::ADV_ENTRIES && gen2::dks_actions(&g.dks, e).iter().all(safe),
                    // No DKS table in the file: keep what the keyboard has.
                    gen2::kind::DKS => {
                        if a != cur {
                            dks += 1;
                        }
                        self.keymap[l][i] = cur;
                        continue;
                    }
                    gen2::kind::MT | gen2::kind::SOCD | gen2::kind::RS | gen2::kind::OKS => {
                        mt_ok && 6 * e + 6 <= g.mt.len() && {
                            let (x, y) = gen2::mt_entry(&g.mt, e);
                            safe(&x) && safe(&y)
                        }
                    }
                    gen2::kind::TGL => tgl_ok && 3 * e + 3 <= g.tgl.len() && safe(&gen2::tgl_entry(&g.tgl, e)),
                    gen2::kind::MACRO => a.code2 == 1 && macros_in.as_ref().is_some_and(|m| m.get(e).is_some_and(|v| !v.is_empty())),
                    _ => safe(&a),
                };
                self.keymap[l][i] = if ok {
                    a
                } else {
                    reset += 1;
                    fac
                };
            }
        }
        if mt_ok {
            self.mt = g.mt.clone();
        }
        if tgl_ok {
            self.tgl = g.tgl.clone();
        }
        if dks_ok {
            self.dks = g.dks.clone();
        }
        if let Some(m) = macros_in {
            self.macros = m;
            self.macro_edit = None;
        }
        if reset > 0 {
            notes.push(format!("{} not offered by Kraken, set back to factory", plural(reset, "key action", "key actions")));
        }
        if dks > 0 {
            notes.push(format!("{} left unchanged (this back-up has no DKS data)", plural(dks, "DKS key", "DKS keys")));
        }

        // Keyboard settings and lighting.
        let mut bad = 0;
        for (name, field) in BACKUP_FIELDS {
            if let Some(&v) = g.settings.get(*name) {
                if backup_value_ok(*field, v) || v == func::get(&self.func, *field) {
                    func::set(&mut self.func, *field, v);
                } else {
                    bad += 1;
                }
            }
        }
        if bad > 0 {
            notes.push(format!("{} not recognised, left unchanged", plural(bad, "setting", "settings")));
        }
        let b = func::get(&self.func, func::LIGHT_BRIGHTNESS);
        if b > 0 {
            self.last_brightness = b;
        }
        if g.colors.is_empty() {
            // No key colours in the file (official driver export): keep them.
        } else if g.colors.len() == self.colors.len() {
            if g.colors != self.colors {
                let before = std::mem::replace(&mut self.colors, g.colors.clone());
                self.color_undo.push(before);
            }
        } else {
            notes.push("key colours are for a different layout, left unchanged".into());
        }
        Ok(notes)
    }

    /// Puts a preset's effect fields and key colours in place; `live_lighting` writes them.
    fn apply_light_preset(&mut self, p: &LightPreset2) {
        if p.fields.len() == LIGHTING.len() || p.fields.len() + 1 == LIGHTING.len() {
            for (f, v) in LIGHTING.iter().zip(&p.fields) {
                func::set(&mut self.func, *f, *v);
            }
            let b = func::get(&self.func, func::LIGHT_BRIGHTNESS);
            if b > 0 {
                self.last_brightness = b;
            }
        }
        if p.colors.len() == self.colors.len() && p.colors != self.colors {
            let before = std::mem::replace(&mut self.colors, p.colors.clone());
            self.color_undo.push(before);
            if self.color_undo.len() > 30 {
                self.color_undo.remove(0);
            }
        }
        self.light_due = Some(Instant::now());
    }

    fn key_action(&self, layer: usize, i: usize) -> Option<Action> {
        let slot = self.slots.get(i).copied().flatten()? as usize;
        self.keymap.get(layer)?.get(slot).copied()
    }

    fn factory_action(&self, layer: usize, i: usize) -> Option<Action> {
        let slot = self.slots.get(i).copied().flatten()? as usize;
        self.snap.as_ref()?.factory.get(layer)?.get(slot).copied()
    }

    /// What a key normally types: its Main-layer action, or for advanced
    /// keys (types 144..149) the factory action.
    fn base_action(&self, slot: usize) -> Action {
        let a = self.keymap.first().and_then(|l| l.get(slot)).copied().unwrap_or_default();
        if (gen2::kind::DKS..=gen2::kind::OKS).contains(&a.kind) {
            self.snap.as_ref().and_then(|s| s.factory[0].get(slot)).copied().unwrap_or_default()
        } else {
            a
        }
    }

    fn advanced_tag(&self, slot: usize) -> Option<&'static str> {
        match self.keymap.first()?.get(slot)?.kind {
            gen2::kind::DKS => Some("DKS"),
            gen2::kind::TGL => Some("TGL"),
            gen2::kind::MT => Some("MT"),
            gen2::kind::RS => Some("RS"),
            gen2::kind::SOCD => Some("SOCD"),
            gen2::kind::OKS => Some("OKS"),
            gen2::kind::MACRO => Some("MACRO"),
            _ => None,
        }
    }

    fn keyboard(&mut self, ui: &mut egui::Ui) {
        let live_view = self.live_view();
        let live_now = Instant::now();
        let unit = theme::key_unit(ui);
        let size = Vec2::new(16.25 * unit, 6.4 * unit);
        let (resp, painter) = ui.allocate_painter(size, Sense::click());
        theme::plate(&painter, resp.rect);
        let origin = resp.rect.min;
        let gap = (unit * 0.06).max(2.0);
        let additive = ui.input(|i| i.modifiers.ctrl || i.modifiers.command || i.modifiers.shift);
        let (small, big, tiny) = (theme::key_font(unit, 0.22, 9.0, 13.0), theme::key_font(unit, 0.26, 10.0, 15.0), theme::key_font(unit, 0.16, 8.0, 11.0));
        let teal = pal().key_tag;
        let mut clicked = None;
        let mut right_clicked = None;
        let mut rects = Vec::with_capacity(self.layout.len());
        for (i, k) in self.layout.iter().enumerate() {
            let rect = Rect::from_min_size(
                origin + Vec2::new(k.x * unit + gap / 2.0, k.y * unit + gap / 2.0),
                Vec2::new(k.w * unit - gap, k.h * unit - gap),
            );
            rects.push(rect);
            let slot = self.slots.get(i).copied().flatten().map(|s| s as usize);
            let r = ui.interact(rect, ui.id().with(("g2key", i)), if slot.is_some() { Sense::click() } else { Sense::hover() });
            if r.clicked() {
                clicked = Some(i);
            }
            if r.secondary_clicked() {
                right_clicked = Some(i);
            }
            let selected = match self.tab {
                Tab::Advanced => self.draft_keys().contains(&i),
                Tab::Lighting | Tab::Profiles | Tab::Settings => false,
                _ => self.knob_sel.is_none() && self.selected.contains(&i),
            };
            let key_color = match (self.tab == Tab::Lighting, live_view) {
                (true, false) => slot.and_then(|s| self.colors.get(s)).map(|c| Color32::from_rgb(c[0], c[1], c[2])),
                (true, true) => slot.and_then(|s| self.led_rgb(s, live_now)).map(|c| Color32::from_rgb(c[0], c[1], c[2])),
                _ => None,
            };
            let tag = slot.and_then(|s| self.advanced_tag(s));
            let edge = (tag.is_some() && self.tab == Tab::Advanced).then_some(teal);
            let cap = Cap { fill: key_color, selected, hovered: r.hovered(), edge, empty: slot.is_none() };
            // Text goes on the top face.
            let rect = theme::keycap(&painter, rect, cap);
            let pad = unit * 0.06;
            let max_w = rect.width() - 2.0 * pad;
            let label = slot.map(|s| gen2::label(&self.base_action(s))).filter(|s| !s.is_empty()).unwrap_or_else(|| k.name.clone());
            let top_left = rect.left_top() + Vec2::splat(pad);
            match self.tab {
                Tab::Performance => {
                    let name = fit_text(&painter, top_left, Align2::LEFT_TOP, &label, max_w, small, pal().key_legend);
                    if let Some(t) = slot.and_then(|s| self.triggers.get(s)) {
                        let ap = t.actuation_mm();
                        let value = painter.text(rect.left_bottom() + Vec2::new(pad, -pad), Align2::LEFT_BOTTOM, format!("{ap:.2}"), FontId::proportional(big), ap_color(ap));
                        let tip = if t.mode() != RtMode::Off {
                            let c = if t.mode() == RtMode::Continuous { "C" } else { "" };
                            // Press / release between name and value, only where it fits; the tooltip has all values.
                            let space = Rect::from_x_y_ranges(name.left()..=rect.right() - pad, name.bottom() + 1.0..=value.top() - 1.0);
                            let text = format!("{} {}{c}", theme::mm(t.press_mm()), theme::mm(t.release_mm()));
                            theme::key_text_if_fits(&painter, space, text, FontId::proportional(tiny), pal().muted);
                            let mode = if t.mode() == RtMode::Continuous { "continuous Rapid Trigger" } else { "Rapid Trigger" };
                            format!("{label}: actuation {} mm, {mode} press {}, release {}", theme::mm(ap), theme::mm(t.press_mm()), theme::mm(t.release_mm()))
                        } else {
                            format!("{label}: actuation {} mm", theme::mm(ap))
                        };
                        let _ = r.clone().on_hover_text(tip);
                    }
                }
                Tab::Keymap => {
                    fit_text(&painter, top_left, Align2::LEFT_TOP, &k.name, max_w, small, pal().muted);
                    let a = self.key_action(self.layer, i).unwrap_or_default();
                    let changed = self.factory_action(self.layer, i).is_some_and(|f| f != a);
                    let text = match gen2::label(&a) {
                        t if t.is_empty() => "—".to_string(),
                        t => t,
                    };
                    let color = if changed { pal().key_tag } else { pal().text };
                    fit_text(&painter, rect.center() + Vec2::new(0.0, unit * 0.12), Align2::CENTER_CENTER, &text, max_w, big, color);
                }
                Tab::Advanced => {
                    fit_text(&painter, top_left, Align2::LEFT_TOP, &label, max_w, small, pal().key_legend);
                    if let Some(t) = tag {
                        painter.text(rect.left_bottom() + Vec2::new(pad, -pad), Align2::LEFT_BOTTOM, t, FontId::proportional(tiny), teal);
                    }
                }
                Tab::Lighting => {
                    let ink = key_color.map_or(pal().key_legend, theme::ink_on);
                    fit_text(&painter, top_left, Align2::LEFT_TOP, &label, max_w, small, ink);
                }
                Tab::Profiles | Tab::Settings => {}
            }
        }

        // Knob (top right): press in the middle, turn left / right on the sides.
        let center = origin + Vec2::new(15.5 * unit, 0.5 * unit);
        let radius = unit * 0.42;
        let knob_rect = Rect::from_center_size(center, Vec2::splat(radius * 2.0));
        let kr = ui.interact(knob_rect, ui.id().with("g2knob"), Sense::click());
        painter.circle(center, radius, if kr.hovered() { pal().key_hover } else { pal().key_skirt }, Stroke::new(2.0_f32, pal().key_top));
        if self.tab == Tab::Keymap {
            for (zone, dx) in [(1usize, -0.62), (0, 0.0), (2, 0.62)] {
                let on = self.knob_sel == Some(zone);
                let p = center + Vec2::new(dx * radius, 0.0);
                if on {
                    painter.circle_filled(p, radius * 0.32, pal().accent);
                }
                let t = ["P", "L", "R"][zone];
                painter.text(p, Align2::CENTER_CENTER, t, FontId::proportional(tiny), if on { pal().on_accent } else { pal().muted });
            }
            if kr.clicked() {
                let dx = kr.interact_pointer_pos().map_or(0.0, |p| p.x - center.x);
                self.knob_sel = Some(if dx < -radius / 3.0 {
                    1
                } else if dx > radius / 3.0 {
                    2
                } else {
                    0
                });
                self.selected.clear();
            }
        }

        match self.tab {
            Tab::Lighting if live_view => {} // live view: nothing to edit on the keys
            Tab::Lighting => {
                // Brush: press and drag across keys to paint, right-click picks a key's colour.
                if let Some(s) = right_clicked.and_then(|i| self.slots[i]) {
                    self.paint = self.colors[s as usize];
                }
                self.paint_stroke(ui, resp.rect, &rects, unit);
            }
            Tab::Keymap => {
                if let Some(i) = clicked {
                    self.knob_sel = None;
                    let only = self.selected.len() == 1 && self.selected.contains(&i);
                    self.selected.clear();
                    if !only {
                        self.selected.insert(i);
                    }
                } else if resp.clicked() && !kr.clicked() {
                    self.selected.clear();
                    self.knob_sel = None;
                }
            }
            Tab::Advanced => {
                if let Some(i) = clicked {
                    self.draft_pick(i);
                }
            }
            Tab::Performance => match clicked {
                Some(i) if additive => {
                    if !self.selected.remove(&i) {
                        self.selected.insert(i);
                    }
                }
                Some(i) => {
                    let only = self.selected.len() == 1 && self.selected.contains(&i);
                    self.selected.clear();
                    if !only {
                        self.selected.insert(i);
                    }
                }
                None if resp.clicked() && !additive => self.selected.clear(),
                None => {}
            },
            Tab::Profiles | Tab::Settings => {}
        }
    }

    /// Custom-lighting brush. A stroke starts with a primary press on the
    /// keyboard (not under a popup) and paints every key the pointer crosses
    /// until release; one stroke = one undo step = one live write.
    fn paint_stroke(&mut self, ui: &egui::Ui, area: Rect, rects: &[Rect], unit: f32) {
        let (pos, down, pressed) = ui.input(|i| (i.pointer.latest_pos(), i.pointer.primary_down(), i.pointer.primary_pressed()));
        if self.painting.is_none() && pressed && ui.rect_contains_pointer(area) {
            self.color_undo.push(self.colors.clone());
            if self.color_undo.len() > 30 {
                self.color_undo.remove(0);
            }
            self.painting = pos;
        }
        let Some(prev) = self.painting else { return };
        if let Some(p) = pos {
            // Sample the segment so fast drags don't skip keys.
            let steps = ((p - prev).length() / (unit * 0.2)).ceil().max(1.0) as usize;
            let mut changed = false;
            for n in 0..=steps {
                let q = prev + (p - prev) * (n as f32 / steps as f32);
                let slot = rects.iter().position(|r| r.contains(q)).and_then(|i| self.slots.get(i).copied().flatten());
                if let Some(c) = slot.and_then(|s| self.colors.get_mut(s as usize)) {
                    if *c != self.paint {
                        *c = self.paint;
                        changed = true;
                    }
                }
            }
            if changed {
                self.light_due = Some(Instant::now());
            }
            self.painting = Some(p);
        }
        if !down {
            self.painting = None;
            if self.color_undo.last() == Some(&self.colors) {
                self.color_undo.pop();
            }
        }
    }

    fn selected_slots(&self) -> Vec<usize> {
        self.selected.iter().filter_map(|i| self.slots.get(*i).copied().flatten()).map(|s| s as usize).collect()
    }

    fn performance(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label("Select:");
            if ui.button("All").clicked() {
                self.selected = (0..self.layout.len()).filter(|i| self.slots[*i].is_some()).collect();
            }
            if ui.button("WASD").clicked() {
                self.selected = self.layout.iter().enumerate().filter(|(_, k)| matches!(k.name.as_str(), "W" | "A" | "S" | "D")).map(|(i, _)| i).collect();
            }
            if ui.button("None").clicked() {
                self.selected.clear();
            }
            theme::help(ui, "Click a key; Ctrl/Shift+click to add.");
        });
        ui.separator();
        let sel = self.selected_slots();
        if sel.is_empty() {
            ui.label("Select keys to edit actuation, Rapid Trigger and dead zones.");
            return;
        }
        let first = self.triggers[sel[0]];
        ui.label(format!("{} key(s) selected · switch: {}", sel.len(), gen2::SWITCHES.get(first.switch_type() as usize).map_or("?", |s| s.0)));

        let mut mode = first.mode();
        ui.horizontal(|ui| {
            ui.add_sized([150.0, 18.0], egui::Label::new("Rapid Trigger"));
            for m in RtMode::ALL {
                if ui.selectable_value(&mut mode, m, m.name()).changed() {
                    for &s in &sel {
                        self.triggers[s].set_mode(mode);
                    }
                }
            }
        });

        ui.spacing_mut().slider_width = (ui.available_width() - 330.0).clamp(160.0, 520.0);
        let max_ap = sel.iter().map(|&s| gen2::max_actuation_mm(&self.triggers[s])).fold(gen2::ACTUATION_MM.1, f32::min);
        type Row = (&'static str, fn(&Trigger) -> f32, fn(&mut Trigger, f32), (f32, f32), bool);
        let rows: [Row; 5] = [
            ("Actuation point", Trigger::actuation_mm, Trigger::set_actuation_mm, (gen2::ACTUATION_MM.0, max_ap), true),
            ("RT press sensitivity", Trigger::press_mm, Trigger::set_press_mm, gen2::RT_MM, mode != RtMode::Off),
            ("RT release sensitivity", Trigger::release_mm, Trigger::set_release_mm, gen2::RT_MM, mode != RtMode::Off),
            ("Top dead zone", Trigger::top_dz_mm, Trigger::set_top_dz_mm, gen2::DEADZONE_MM, true),
            ("Bottom dead zone", Trigger::bottom_dz_mm, Trigger::set_bottom_dz_mm, gen2::DEADZONE_MM, true),
        ];
        let mut edited = false;
        for (name, get, set, (lo, hi), enabled) in rows {
            let mut v = get(&first);
            let mixed = sel.iter().any(|&s| (get(&self.triggers[s]) - v).abs() > 0.004);
            let label = if mixed { format!("{name} (mixed)") } else { name.to_string() };
            ui.add_enabled_ui(enabled, |ui| {
                ui.horizontal(|ui| {
                    ui.add_sized([150.0, 18.0], egui::Label::new(label));
                    let r = ui.add(egui::Slider::new(&mut v, lo..=hi.max(lo)).step_by(0.01).fixed_decimals(2).suffix(" mm"));
                    if r.changed() {
                        edited = true;
                        for &s in &sel {
                            set(&mut self.triggers[s], v);
                        }
                    }
                });
            });
        }
        // Keep actuation inside travel minus bottom dead zone (only after an edit,
        // so selecting a key never changes it by itself).
        for &s in sel.iter().filter(|_| edited) {
            let m = gen2::max_actuation_mm(&self.triggers[s]);
            if self.triggers[s].actuation_mm() > m + 0.001 {
                self.triggers[s].set_actuation_mm(m);
            }
        }
    }

    fn keymap_editor(&mut self, ui: &mut egui::Ui) {
        const LAYERS: [&str; 4] = ["Main", "FN1", "FN2", "FN3"];
        ui.horizontal(|ui| {
            ui.label("Layer:");
            for (i, n) in LAYERS.iter().enumerate() {
                ui.selectable_value(&mut self.layer, i, *n);
            }
            ui.separator();
            if ui.button(format!("Reset {} layer to factory", LAYERS[self.layer])).clicked() {
                if let (Some(w), Some(f)) = (self.keymap.get_mut(self.layer), self.snap.as_ref().and_then(|s| s.factory.get(self.layer))) {
                    *w = f.clone();
                }
            }
        });
        let missing = self.slots.iter().filter(|s| s.is_none()).count();
        if missing > 0 {
            ui.colored_label(pal().unsaved, format!("{missing} keys could not be matched to the keyboard's factory map (shown dark)."));
        }
        ui.horizontal(|ui| {
            ui.label("Knob:");
            for z in 0..3 {
                let Some(s) = self.knob[z] else { continue };
                let a = self.keymap[self.layer][s as usize];
                let text = format!("{}: {}", ["Press", "Left", "Right"][z], gen2::label(&a));
                if ui.add(egui::Button::new(text).selected(self.knob_sel == Some(z))).clicked() {
                    self.knob_sel = Some(z);
                    self.selected.clear();
                }
            }
        });
        ui.separator();
        let target = match self.knob_sel {
            Some(z) => self.knob[z].map(|s| (s as usize, KNOB_NAMES[z].to_string())),
            None => self.selected.iter().next().and_then(|&i| self.slots.get(i).copied().flatten().map(|s| (s as usize, self.layout[i].name.clone()))),
        };
        let Some((slot, title)) = target else {
            ui.label("Click a key (or the knob), then pick what it should send. Keep an FN key on the Main layer to reach the FN layers.");
            return;
        };
        let current = self.keymap[self.layer][slot];
        let factory = self.snap.as_ref().and_then(|s| s.factory.get(self.layer)?.get(slot).copied()).unwrap_or_default();
        let name = |a: &Action| {
            let l = gen2::label(a);
            if l.is_empty() {
                "nothing".to_string()
            } else {
                l
            }
        };
        ui.horizontal(|ui| {
            ui.strong(format!("{title} · {} layer", LAYERS[self.layer]));
            ui.label(format!("sends: {}   (factory: {})", name(&current), name(&factory)));
            if ui.add_enabled(current != factory, egui::Button::new("Restore")).clicked() {
                self.keymap[self.layer][slot] = factory;
            }
            if ui.add_enabled(current != Action::default(), egui::Button::new("Clear")).clicked() {
                self.keymap[self.layer][slot] = Action::default();
            }
        });
        // The page scrolls; no inner scroll area, so the keyboard keeps its size.
        ui.scope(|ui| {
            for (cat, items) in gen2::picker() {
                egui::CollapsingHeader::new(*cat).default_open(true).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for item in items {
                            let b = egui::Button::new(&item.label).selected(item.action == current).min_size(Vec2::new(40.0, 24.0));
                            if ui.add(b).clicked() {
                                self.keymap[self.layer][slot] = item.action;
                            }
                        }
                    });
                });
            }
        });
    }

    // ------------------------------------------------------------ advanced keys

    fn draft_keys(&self) -> Vec<usize> {
        match self.draft {
            Some(Draft::Pair { keys, .. }) => keys.iter().flatten().copied().collect(),
            Some(Draft::Dks { slot, .. }) => slot.and_then(|s| self.slots.iter().position(|x| *x == Some(s as u8))).into_iter().collect(),
            Some(Draft::ModTap { key, .. }) | Some(Draft::Toggle { key, .. }) => key.into_iter().collect(),
            Some(Draft::MacroBind { .. }) | None => Vec::new(),
        }
    }

    fn draft_pick(&mut self, i: usize) {
        let Some(slot) = self.slots[i].map(|s| s as usize) else { return };
        if matches!(self.draft, Some(Draft::Dks { entry: Some(_), .. })) {
            return; // editing an existing DKS key: the key is fixed
        }
        if self.draft.is_some() && self.advanced_tag(slot).is_some() {
            self.set_status(format!("{} is already an advanced key — remove it first", self.layout[i].name), true);
            return;
        }
        if let Some(Draft::MacroBind { id }) = self.draft {
            self.keymap[0][slot] = gen2::macro_action(id as u8);
            self.draft = None;
            self.set_status(format!("{} plays M{} — Save to keyboard to apply", self.layout[i].name, id + 1), false);
            return;
        }
        let base = self.base_action(slot);
        match self.draft.as_mut() {
            Some(Draft::Dks { slot: s, .. }) => *s = Some(slot),
            Some(Draft::Pair { keys, .. }) => {
                if keys[0] == Some(i) {
                    keys[0] = None;
                } else if keys[1] == Some(i) {
                    keys[1] = None;
                } else if keys[0].is_none() {
                    keys[0] = Some(i);
                } else {
                    keys[1] = Some(i);
                }
            }
            Some(Draft::ModTap { key, tap, .. }) => {
                *key = Some(i);
                *tap = base;
            }
            Some(Draft::Toggle { key, action }) => {
                *key = Some(i);
                *action = base;
            }
            Some(Draft::MacroBind { .. }) | None => {}
        }
    }

    /// Name of the key (or knob action) at a matrix slot.
    fn slot_name(&self, slot: u8) -> String {
        let i = self.slots.iter().position(|s| *s == Some(slot));
        match (i, self.knob.iter().position(|k| *k == Some(slot))) {
            (Some(i), _) => self.layout[i].name.clone(),
            (None, Some(z)) => KNOB_NAMES[z].to_string(),
            _ => format!("slot {slot}"),
        }
    }

    fn advanced(&mut self, ui: &mut egui::Ui) {
        if self.snap.is_none() {
            return;
        }
        let list = gen2::find_advanced(&self.keymap[0], &self.mt, &self.tgl, &self.dks);
        let name_of = |slot: u8| self.slot_name(slot);
        let lbl = |a: &Action| match gen2::label(a) {
            t if t.is_empty() => "nothing".to_string(),
            t => t,
        };
        ui.label("Advanced keys on the Main layer:");
        let mut remove: Option<Advanced> = None;
        let mut new_priority: Option<(u8, u8, u8)> = None;
        let mut edit: Option<(u8, u8)> = None;
        egui::Grid::new("adv").num_columns(3).spacing([14.0, 6.0]).striped(true).show(ui, |ui| {
            if list.is_empty() {
                ui.label("none");
                ui.end_row();
            }
            for adv in &list {
                match adv {
                    Advanced::Pair { kind, a, b, .. } => {
                        ui.strong(kind.name());
                        ui.label(format!("{} + {}", name_of(*a), name_of(*b)));
                        if *kind == PairKind::Socd {
                            let mut p = self.triggers[*a as usize].priority();
                            let names = ["Last input wins", "First key wins", "Second key wins"];
                            egui::ComboBox::from_id_salt(("prio", *a)).selected_text(*names.get(p as usize).unwrap_or(&"?")).show_ui(ui, |ui| {
                                for (v, n) in names.iter().enumerate() {
                                    ui.selectable_value(&mut p, v as u8, *n);
                                }
                            });
                            if p != self.triggers[*a as usize].priority() {
                                new_priority = Some((*a, *b, p));
                            }
                        } else {
                            ui.label("");
                        }
                    }
                    Advanced::Mt { slot, tap, hold, hold_ms, .. } => {
                        ui.strong("Mod Tap");
                        ui.label(format!("{}: tap {} · hold {} ({hold_ms} ms)", name_of(*slot), lbl(tap), lbl(hold)));
                        ui.label("");
                    }
                    Advanced::Tgl { slot, action, .. } => {
                        ui.strong("Toggle");
                        ui.label(format!("{}: toggles {}", name_of(*slot), lbl(action)));
                        ui.label("");
                    }
                    Advanced::Dks { slot, entry, .. } => {
                        ui.strong("DKS");
                        ui.label(format!("{}: {}", name_of(*slot), dks_summary(&gen2::dks_entry(&self.dks, *entry as usize))));
                        if ui.small_button("Edit").clicked() {
                            edit = Some((*slot, *entry));
                        }
                    }
                }
                if ui.small_button("Remove").clicked() {
                    remove = Some(adv.clone());
                }
                ui.end_row();
            }
        });
        if let Some((a, b, p)) = new_priority {
            self.triggers[a as usize].set_priority(p);
            self.triggers[b as usize].set_priority([0, 2, 1][p.min(2) as usize]);
        }
        if let Some((slot, entry)) = edit {
            self.draft = Some(Draft::Dks { slot: Some(slot as usize), entry: Some(entry as usize), dks: gen2::dks_entry(&self.dks, entry as usize) });
        }
        if let Some(adv) = remove {
            self.remove_advanced(&adv);
        }

        ui.add_space(8.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Add:");
            for (kind, name) in [(PairKind::Socd, "SOCD pair"), (PairKind::Rs, "Rappy Snappy"), (PairKind::Oks, "One Key Switch")] {
                if ui.button(name).clicked() {
                    self.draft = Some(Draft::Pair { kind, keys: [None, None], priority: 0 });
                }
            }
            if ui.button("Mod Tap").clicked() {
                self.draft = Some(Draft::ModTap { key: None, tap: Action::default(), hold: Action { kind: gen2::kind::KEY, code1: 1, code2: 0 }, hold_ms: 200 });
            }
            if ui.button("Toggle").clicked() {
                self.draft = Some(Draft::Toggle { key: None, action: Action::default() });
            }
            if ui.button("DKS").clicked() {
                self.draft = Some(Draft::Dks { slot: None, entry: None, dks: Dks::default() });
            }
        });
        let Some(mut draft) = self.draft else {
            theme::help(ui, "SOCD: two keys, the last one pressed wins (or a fixed priority). Rappy Snappy: two keys, the one pressed deeper wins. One Key Switch: two keys, letting go of one presses the other.");
            theme::help(ui, "Mod Tap: tap for one action, hold for another. Toggle: one press holds an action until pressed again. DKS: up to four actions at different depths of one key press.");
            return;
        };
        let key_name = |i: Option<usize>| i.map_or("click a key".to_string(), |i| self.layout[i].name.clone());
        let slot_name = |s: Option<usize>| s.map_or("click a key".to_string(), |s| self.slot_name(s as u8));
        let mut cancel = false;
        let create = match &mut draft {
            Draft::Pair { kind, keys, priority } => {
                ui.label(format!("{}: {} + {}", kind.name(), key_name(keys[0]), key_name(keys[1])));
                match kind {
                    PairKind::Socd => {
                        ui.horizontal(|ui| {
                            ui.label("Priority");
                            for (v, n) in ["Last input wins", "First key wins", "Second key wins"].iter().enumerate() {
                                ui.selectable_value(priority, v as u8, *n);
                            }
                        });
                    }
                    PairKind::Rs => theme::help(ui, "With both keys down, only the one pressed deeper counts; both pressed all the way count together. Key travel settings stay as they are."),
                    PairKind::Oks => theme::help(ui, "Letting go of one key presses the other for you."),
                }
                ui.add_enabled(keys.iter().all(Option::is_some), egui::Button::new("Create")).clicked()
            }
            Draft::Dks { slot, entry, dks } => {
                ui.label(format!("DKS on {}", slot_name(*slot)));
                dks_editor(ui, dks);
                let ready = slot.is_some() && !dks.is_empty();
                ui.add_enabled(ready, egui::Button::new(if entry.is_some() { "Apply" } else { "Create" })).clicked()
            }
            Draft::ModTap { key, tap, hold, hold_ms } => {
                ui.label(format!("Mod Tap on {}", key_name(*key)));
                action_combo(ui, "mt_tap", "Tap", tap);
                action_combo(ui, "mt_hold", "Hold", hold);
                ui.horizontal(|ui| {
                    ui.add_sized([60.0, 18.0], egui::Label::new("Hold time"));
                    ui.add(egui::Slider::new(hold_ms, 100..=1000).step_by(10.0).suffix(" ms"));
                });
                ui.add_enabled(key.is_some(), egui::Button::new("Create")).clicked()
            }
            Draft::Toggle { key, action } => {
                ui.label(format!("Toggle on {}", key_name(*key)));
                action_combo(ui, "tgl_action", "Toggles", action);
                ui.add_enabled(key.is_some(), egui::Button::new("Create")).clicked()
            }
            Draft::MacroBind { id } => {
                ui.label(format!("Click the key that should play M{}.", *id + 1));
                false
            }
        };
        if ui.button("Cancel").clicked() {
            cancel = true;
        }
        self.draft = Some(draft);
        if cancel {
            self.draft = None;
        } else if create {
            match self.create_advanced(draft) {
                Ok(msg) => {
                    self.set_status(format!("{msg} — Save to keyboard to apply"), false);
                    self.draft = None;
                }
                Err(e) => self.set_status(e, true),
            }
        }
    }

    fn create_advanced(&mut self, d: Draft) -> Result<String, String> {
        let slot = |i: Option<usize>| -> Result<usize, String> { i.and_then(|i| self.slots[i]).map(|s| s as usize).ok_or("pick a key".to_string()) };
        match d {
            Draft::Pair { kind, keys, priority } => {
                let (a, b) = (slot(keys[0])?, slot(keys[1])?);
                let k = gen2::free_entries(&gen2::mt_entries_used(&self.keymap), 2).ok_or("advanced key table is full")?;
                let (aa, ab) = (self.base_action(a), self.base_action(b));
                gen2::set_mt_entry(&mut self.mt, k, aa, ab);
                gen2::set_mt_entry(&mut self.mt, k + 1, ab, aa);
                self.keymap[0][a] = Action { kind: kind.kind(), code1: k as u8, code2: b as u8 };
                self.keymap[0][b] = Action { kind: kind.kind(), code1: k as u8 + 1, code2: a as u8 };
                // The priority nibble belongs to SOCD; other pairs leave it at "last input".
                let priority = if kind == PairKind::Socd { priority.min(2) } else { 0 };
                self.triggers[a].set_priority(priority);
                self.triggers[b].set_priority([0, 2, 1][priority as usize]);
                Ok(format!("{} pair added", kind.name()))
            }
            Draft::Dks { slot, entry, mut dks } => {
                let s = slot.ok_or("pick a key")?;
                for b in dks.binds.iter_mut() {
                    if b.action.kind == 0 {
                        b.spans = [0; 4];
                    }
                }
                if dks.is_empty() {
                    return Err("Bind at least one action".into());
                }
                let k = match entry {
                    Some(k) => k,
                    None => gen2::free_entries(&gen2::dks_entries_used(&self.keymap), 1).ok_or("DKS table is full")?,
                };
                gen2::set_dks_entry(&mut self.dks, k, &dks);
                self.keymap[0][s] = Action { kind: gen2::kind::DKS, code1: k as u8, code2: 0 };
                Ok(if entry.is_some() { "DKS key updated" } else { "DKS key added" }.into())
            }
            Draft::ModTap { key, tap, hold, hold_ms } => {
                let s = slot(key)?;
                let k = gen2::free_entries(&gen2::mt_entries_used(&self.keymap), 1).ok_or("advanced key table is full")?;
                gen2::set_mt_entry(&mut self.mt, k, tap, hold);
                self.keymap[0][s] = Action { kind: gen2::kind::MT, code1: k as u8, code2: (hold_ms / 10).min(255) as u8 };
                Ok("Mod Tap added".into())
            }
            Draft::MacroBind { .. } => Err("click a key".into()),
            Draft::Toggle { key, action } => {
                let s = slot(key)?;
                let k = gen2::free_entries(&gen2::tgl_entries_used(&self.keymap), 1).ok_or("toggle table is full")?;
                gen2::set_tgl_entry(&mut self.tgl, k, action);
                self.keymap[0][s] = Action { kind: gen2::kind::TGL, code1: k as u8, code2: 0 };
                Ok("Toggle added".into())
            }
        }
    }

    /// Puts the keys back to their factory Main-layer action and clears the table entries.
    fn remove_advanced(&mut self, adv: &Advanced) {
        let factory = |s: u8| self.snap.as_ref().map_or(Action::default(), |n| n.factory[0][s as usize]);
        match adv {
            Advanced::Pair { a, b, entry, .. } => {
                for (s, e) in [(*a, *entry), (*b, *entry + 1)] {
                    self.keymap[0][s as usize] = factory(s);
                    self.triggers[s as usize].set_priority(0);
                    gen2::set_mt_entry(&mut self.mt, e as usize, Action::default(), Action::default());
                }
            }
            Advanced::Mt { slot, entry, .. } => {
                self.keymap[0][*slot as usize] = factory(*slot);
                gen2::set_mt_entry(&mut self.mt, *entry as usize, Action::default(), Action::default());
            }
            Advanced::Tgl { slot, entry, .. } => {
                self.keymap[0][*slot as usize] = factory(*slot);
                gen2::set_tgl_entry(&mut self.tgl, *entry as usize, Action::default());
            }
            Advanced::Dks { slot, entry, .. } => {
                self.keymap[0][*slot as usize] = factory(*slot);
                gen2::clear_dks_entry(&mut self.dks, *entry as usize);
            }
        }
        if matches!(self.draft, Some(Draft::Dks { entry: Some(_), .. })) {
            self.draft = None;
        }
        self.set_status("Advanced key removed — Save to keyboard to apply", false);
    }

    /// Puts every key that plays macro `id` back to its factory action (all layers).
    fn macro_unbind(&mut self, id: usize) {
        let Some(snap) = &self.snap else { return };
        for (layer, keys) in self.keymap.iter_mut().enumerate() {
            for (slot, a) in keys.iter_mut().enumerate() {
                if a.kind == gen2::kind::MACRO && a.code1 as usize == id {
                    *a = snap.factory.get(layer).and_then(|l| l.get(slot)).copied().unwrap_or_default();
                }
            }
        }
    }

    /// Macros: list, recorder / editor, and putting a macro on a key.
    fn macros_card(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Macros");
        if self.macros.is_empty() {
            theme::help(ui, "This keyboard didn't answer when asked for its macros, so they are switched off here.");
            return;
        }
        let total: usize = self.macros.iter().map(Vec::len).sum();
        let (mut edit, mut bind, mut unbind, mut delete) = (None, None, None, None);
        egui::Grid::new("macros").num_columns(6).spacing([14.0, 6.0]).striped(true).show(ui, |ui| {
            for (id, events) in self.macros.iter().enumerate().filter(|(_, e)| !e.is_empty()) {
                let keys: Vec<String> = self
                    .keymap
                    .iter()
                    .flat_map(|l| l.iter().enumerate())
                    .filter(|(_, a)| a.kind == gen2::kind::MACRO && a.code1 as usize == id)
                    .map(|(slot, _)| self.slot_name(slot as u8))
                    .collect();
                ui.strong(format!("M{}", id + 1));
                ui.label(macro_summary(events));
                ui.label(if keys.is_empty() { "not on a key".to_string() } else { format!("on {}", keys.join(", ")) });
                if ui.small_button("Edit").clicked() {
                    edit = Some(id);
                }
                if ui.small_button("Put on a key").clicked() {
                    bind = Some(id);
                }
                if !keys.is_empty() && ui.small_button("Take off keys").clicked() {
                    unbind = Some(id);
                }
                if ui.small_button("Delete").clicked() {
                    delete = Some(id);
                }
                ui.end_row();
            }
        });
        if total == 0 {
            ui.label("No macros yet.");
        }
        let free_id = (0..gen2::MACROS).find(|&id| self.macros[id].is_empty());
        if ui.add_enabled(free_id.is_some() && self.macro_edit.is_none(), egui::Button::new("New macro")).clicked() {
            edit = free_id;
        }
        if let Some(id) = edit {
            self.macro_edit = Some(MacroEdit { id, events: self.macros[id].clone(), recording: false, timed: false, last_event: None, mods: egui::Modifiers::NONE });
        }
        if let Some(id) = bind {
            self.draft = Some(Draft::MacroBind { id });
            self.set_status(format!("Click the key that should play M{}", id + 1), false);
        }
        if let Some(id) = unbind.or(delete) {
            self.macro_unbind(id);
        }
        if let Some(id) = delete {
            self.macros[id].clear();
            if self.macro_edit.as_ref().is_some_and(|e| e.id == id) {
                self.macro_edit = None;
            }
            self.set_status(format!("M{} deleted — Save to keyboard to apply", id + 1), false);
        }

        let Some(mut e) = self.macro_edit.take() else {
            theme::help(ui, "A macro is a list of key presses the keyboard plays back when you press the key it is on. Macros belong to the profile.");
            return;
        };
        ui.add_space(6.0);
        ui.separator();
        // Room for this macro: the block holds 496 events for all macros together.
        let room = gen2::MACRO_EVENTS_MAX - (total - self.macros[e.id].len());
        match macro_editor(ui, &mut e, room) {
            Some(true) => {
                if let Some(last) = e.events.last_mut() {
                    last.delay_ms = 0;
                }
                self.macros[e.id] = e.events;
                self.set_status(format!("M{} updated — Save to keyboard to apply", e.id + 1), false);
            }
            Some(false) => {}
            None => self.macro_edit = Some(e),
        }
    }

    /// Lighting tab; any change to the effect fields is written live.
    fn lighting(&mut self, ui: &mut egui::Ui) {
        let before: Vec<u8> = LIGHTING.iter().map(|f| func::get(&self.func, *f)).collect();
        self.lighting_controls(ui);
        if LIGHTING.iter().map(|f| func::get(&self.func, *f)).ne(before.iter().copied()) {
            self.light_due = Some(Instant::now() + Duration::from_millis(150));
        }
    }

    fn lighting_controls(&mut self, ui: &mut egui::Ui) {
        // Fields are written back only when the user changes them, so values
        // outside the widget ranges never turn into phantom edits.
        let effect = func::get(&self.func, func::LIGHT_EFFECT);
        let bright = func::get(&self.func, func::LIGHT_BRIGHTNESS);
        if effect == 0 {
            self.custom_colors(ui);
            ui.separator();
        }
        let mut on = bright > 0;
        if ui.checkbox(&mut on, "Backlight").changed() {
            func::set(&mut self.func, func::LIGHT_BRIGHTNESS, if on { self.last_brightness.max(10) } else { 0 });
        }
        if !on {
            return;
        }
        ui.add_space(4.0);
        ui.label("Effect");
        ui.horizontal_wrapped(|ui| {
            for (i, n) in gen2::EFFECTS.iter().enumerate() {
                if ui.add(egui::Button::new(*n).selected(effect == i as u8).min_size(Vec2::new(86.0, 26.0))).clicked() && effect != i as u8 {
                    func::set(&mut self.func, func::LIGHT_EFFECT, i as u8);
                }
            }
        });
        ui.add_space(6.0);
        ui.spacing_mut().slider_width = 260.0;
        let mut b = bright;
        ui.horizontal(|ui| {
            ui.add_sized([120.0, 18.0], egui::Label::new("Brightness"));
            if ui.add(egui::Slider::new(&mut b, 1..=100)).changed() {
                func::set(&mut self.func, func::LIGHT_BRIGHTNESS, b);
                self.last_brightness = b;
            }
        });
        if gen2::effect_has_speed(effect) {
            let mut speed = 4u8.saturating_sub(func::get(&self.func, func::LIGHT_SPEED_INV)).min(4);
            ui.horizontal(|ui| {
                ui.add_sized([120.0, 18.0], egui::Label::new("Speed"));
                if ui.add(egui::Slider::new(&mut speed, 0..=4)).changed() {
                    func::set(&mut self.func, func::LIGHT_SPEED_INV, 4 - speed);
                }
            });
        }
        if let Some(choices) = gen2::effect_directions(effect) {
            let mut dir = func::get(&self.func, func::LIGHT_DIRECTION);
            ui.horizontal(|ui| {
                ui.add_sized([120.0, 18.0], egui::Label::new("Direction"));
                for (value, name) in choices {
                    if ui.selectable_value(&mut dir, value, name).changed() {
                        func::set(&mut self.func, func::LIGHT_DIRECTION, value);
                    }
                }
            });
        }
        if gen2::effect_has_color(effect) {
            ui.horizontal(|ui| {
                ui.add_sized([120.0, 18.0], egui::Label::new("Colour"));
                let mut rgb = [self.func[14], self.func[15], self.func[16]];
                if ui.color_edit_button_srgb(&mut rgb).changed() {
                    self.func[14..17].copy_from_slice(&rgb);
                    func::set(&mut self.func, func::LIGHT_MULTI, 0); // official: picking a colour = single colour
                }
                if func::get(&self.func, func::LIGHT_MULTI) != 0 {
                    theme::help(ui, "(currently multi-colour; picking a colour switches to single colour)");
                }
            });
        }
        theme::help(ui, "Changes apply to the keyboard right away (current profile).");
    }

    /// "Custom" effect: per-key colours painted on the keyboard above.
    fn custom_colors(&mut self, ui: &mut egui::Ui) {
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
            // Fill / Undo first: the colour picker popup opens down-right of
            // its button and must not cover them.
            if ui.button("Fill all keys").clicked() {
                let all = vec![Some(self.paint); self.layout.len()];
                self.set_key_colors("fill all keys", &all);
            }
            if ui.add_enabled(!self.color_undo.is_empty(), egui::Button::new("Undo")).clicked() {
                if let Some(prev) = self.color_undo.pop() {
                    self.colors = prev;
                    self.light_due = Some(Instant::now());
                }
            }
            ui.separator();
            ui.label("Brush");
            ui.color_edit_button_srgb(&mut self.paint);
            for c in SWATCHES {
                let (r, p) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                ui.painter().rect_filled(r, 4.0, Color32::from_rgb(c[0], c[1], c[2]));
                let stroke = if self.paint == c { Stroke::new(2.0_f32, Color32::WHITE) } else { Stroke::new(1.0_f32, pal().input) };
                ui.painter().rect_stroke(r, 4.0, stroke, StrokeKind::Inside);
                if p.clicked() {
                    self.paint = c;
                }
            }
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label("Highlight");
            for g in pattern::Group::ALL {
                if ui.button(g.label()).on_hover_text("Paint these keys with the brush colour").clicked() {
                    let per_key: Vec<_> = self.layout.iter().map(|k| g.contains(k.code).then_some(self.paint)).collect();
                    self.set_key_colors(g.label(), &per_key);
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Pattern");
            egui::ComboBox::from_id_salt("g2pattern_dir").selected_text(self.pattern_dir.label()).show_ui(ui, |ui| {
                for d in pattern::Direction::ALL {
                    ui.selectable_value(&mut self.pattern_dir, d, d.label());
                }
            });
            let centres: Vec<[f32; 2]> = self.layout.iter().map(|k| [k.x + k.w / 2.0, k.y + k.h / 2.0]).collect();
            let mut generated = None;
            if ui.button("Gradient").on_hover_text("Brush colour to the second colour").clicked() {
                generated = Some(("gradient", pattern::gradient(&centres, self.pattern_dir, self.paint, self.paint2)));
            }
            ui.color_edit_button_srgb(&mut self.paint2).on_hover_text("Second colour (gradient, stripes, checker)");
            if ui.button("Stripes").on_hover_text("Brush / second colour: rows, columns, diagonals or rings").clicked() {
                generated = Some(("stripes", pattern::stripes(&centres, self.pattern_dir, self.paint, self.paint2)));
            }
            if ui.button("Checker").on_hover_text("Brush / second colour checkerboard").clicked() {
                generated = Some(("checker", pattern::checker(&centres, self.paint, self.paint2)));
            }
            ui.separator();
            if ui.button("Rainbow").clicked() {
                generated = Some(("rainbow", pattern::rainbow(&centres, self.pattern_dir, 5.0 / 6.0, 0.0)));
            }
            if ui.button("Random").clicked() {
                let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
                generated = Some(("random", pattern::random(centres.len(), seed)));
            }
            ui.separator();
            let name = |i: usize| pattern::PRESETS.get(i).map_or("?", |p| p.0);
            egui::ComboBox::from_id_salt("g2palette").selected_text(name(self.palette)).show_ui(ui, |ui| {
                for i in 0..pattern::PRESETS.len() {
                    ui.selectable_value(&mut self.palette, i, name(i));
                }
            });
            if ui.button("Apply palette").on_hover_text("Three-colour blend along the direction").clicked() {
                if let Some((_, stops)) = pattern::PRESETS.get(self.palette) {
                    generated = Some(("palette", pattern::palette(&centres, self.pattern_dir, stops)));
                }
            }
            if let Some((what, colors)) = generated {
                let per_key: Vec<_> = colors.into_iter().map(Some).collect();
                self.set_key_colors(what, &per_key);
            }
        });
        theme::help(ui, "Custom effect: click or drag across keys to paint, right-click a key to pick its colour. Highlight / Pattern repaint keys at once; Undo steps back one change.");
    }

    /// Sets colours per layout key (`None` = leave as is) as one undo step,
    /// written live.
    fn set_key_colors(&mut self, what: &str, per_key: &[Option<[u8; 3]>]) {
        let before = self.colors.clone();
        for (i, c) in per_key.iter().enumerate() {
            let (Some(c), Some(s)) = (c, self.slots.get(i).copied().flatten()) else { continue };
            if let Some(dst) = self.colors.get_mut(s as usize) {
                *dst = *c;
            }
        }
        let changed = self.colors.iter().zip(&before).filter(|(a, b)| a != b).count();
        crate::log::line(format!("gen2 ui: {what}: {changed} keys changed"));
        if changed > 0 {
            self.color_undo.push(before);
            if self.color_undo.len() > 30 {
                self.color_undo.remove(0);
            }
            self.light_due = Some(Instant::now());
        }
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        let combo = |ui: &mut egui::Ui, id: &str, name: &str, func_block: &mut Vec<u8>, field: func::Field, opts: &[(u8, &str)]| {
            let mut v = func::get(func_block, field);
            let before = v;
            ui.horizontal(|ui| {
                ui.add_sized([150.0, 18.0], egui::Label::new(name));
                let cur = opts.iter().find(|o| o.0 == v).map_or(format!("({v})"), |o| o.1.to_string());
                egui::ComboBox::from_id_salt(id).selected_text(cur).show_ui(ui, |ui| {
                    for (val, label) in opts {
                        ui.selectable_value(&mut v, *val, *label);
                    }
                });
            });
            if v != before {
                func::set(func_block, field, v);
            }
        };
        combo(ui, "g2_poll", "Polling rate", &mut self.func, func::POLLING, func::POLLING_RATES);
        theme::help(ui, "Changing the polling rate makes the keyboard reconnect.");
        combo(ui, "g2_tick", "Tick rate", &mut self.func, func::TICK, func::TICK_RATES);
        combo(ui, "g2_debounce", "Anti-shake level", &mut self.func, func::DEBOUNCE, func::DEBOUNCE_LEVELS);
        ui.add_space(6.0);
        for (field, name, help) in [
            (func::ADAPTIVE, "Adaptive Stable", "Helps switches that disconnect or don't trigger because of tolerances."),
            (func::LOCK_WIN, "Lock Win key", ""),
            (func::LOCK_ALT_TAB, "Lock Alt+Tab", ""),
            (func::LOCK_ALT_F4, "Lock Alt+F4", ""),
        ] {
            let mut on = func::get(&self.func, field) != 0;
            let r = ui.checkbox(&mut on, name);
            if r.changed() {
                func::set(&mut self.func, field, on as u8);
            }
            if !help.is_empty() {
                r.on_hover_text(help);
            }
        }
        ui.add_space(6.0);
        theme::help(ui, "Factory reset, calibration and firmware update are intentionally not available here.");
    }
}

/// Picker for one action (same catalog as the Keymap tab).
fn action_combo(ui: &mut egui::Ui, id: &str, name: &str, action: &mut Action) {
    ui.horizontal(|ui| {
        ui.add_sized([60.0, 18.0], egui::Label::new(name));
        action_pick(ui, id, action);
    });
}

fn action_pick(ui: &mut egui::Ui, id: impl std::hash::Hash, action: &mut Action) {
    let text = match gen2::label(action) {
        t if t.is_empty() => "nothing".to_string(),
        t => t,
    };
    egui::ComboBox::from_id_salt(id).selected_text(text).height(320.0).show_ui(ui, |ui| {
        for (cat, items) in gen2::picker() {
            ui.label(egui::RichText::new(*cat).color(pal().muted));
            for item in items {
                ui.selectable_value(action, item.action, &item.label);
            }
        }
    });
}

fn macro_key_label(k: MacroKey) -> String {
    let a = match k {
        MacroKey::Key(c @ 224..=231) => Action { kind: gen2::kind::KEY, code1: 1 << (c - 224), code2: 0 },
        MacroKey::Key(c) => Action { kind: gen2::kind::KEY, code1: 0, code2: c },
        MacroKey::Mouse(b) => return format!("Mouse {b}"),
    };
    match gen2::label(&a) {
        t if t.is_empty() => format!("key {}", a.code2),
        t => t,
    }
}

/// Keys a macro step can use: the picker's plain keys and modifiers, as HID codes.
fn macro_keys() -> Vec<(&'static str, Vec<(String, u8)>)> {
    gen2::picker()
        .iter()
        .map(|(cat, items)| {
            let keys: Vec<(String, u8)> = items
                .iter()
                .filter_map(|i| match i.action {
                    Action { kind: gen2::kind::KEY, code1: 0, code2 } if code2 != 0 => Some((i.label.clone(), code2)),
                    Action { kind: gen2::kind::KEY, code1, code2: 0 } if code1.is_power_of_two() => Some((i.label.clone(), 224 + code1.trailing_zeros() as u8)),
                    _ => None,
                })
                .collect();
            (*cat, keys)
        })
        .filter(|(_, keys)| !keys.is_empty())
        .collect()
}

/// `6 steps: LCtrl C` — the keys pressed, in order.
fn macro_summary(events: &[MacroEvent]) -> String {
    let pressed: Vec<String> = events.iter().filter(|e| e.press).map(|e| macro_key_label(e.key)).collect();
    let shown = pressed.iter().take(10).cloned().collect::<Vec<_>>().join(" ");
    format!("{} steps: {shown}{}", events.len(), if pressed.len() > 10 { " …" } else { "" })
}

/// HID usage of a key egui reports (letters, digits, F1–F12 by name; the rest listed).
fn hid_of(key: egui::Key) -> Option<u8> {
    use egui::Key::*;
    Some(match key {
        Enter => 40,
        Escape => 41,
        Backspace => 42,
        Tab => 43,
        Space => 44,
        Minus => 45,
        Equals => 46,
        OpenBracket => 47,
        CloseBracket => 48,
        Backslash => 49,
        Semicolon => 51,
        Quote => 52,
        Backtick => 53,
        Comma => 54,
        Period => 55,
        Slash => 56,
        Insert => 73,
        Home => 74,
        PageUp => 75,
        Delete => 76,
        End => 77,
        PageDown => 78,
        ArrowRight => 79,
        ArrowLeft => 80,
        ArrowDown => 81,
        ArrowUp => 82,
        k => match k.name().as_bytes() {
            [c @ b'A'..=b'Z'] => 4 + (c - b'A'),
            [b'0'] => 39,
            [c @ b'1'..=b'9'] => 30 + (c - b'1'),
            [b'F', d @ b'1'..=b'9'] => 58 + (d - b'1'),
            [b'F', b'1', d @ b'0'..=b'2'] => 67 + (d - b'0'),
            _ => return None,
        },
    })
}

/// Recorder: turns this frame's key events into macro steps. Ctrl / Shift / Alt
/// are taken from the modifier state (egui has no key events for them) as the left keys.
fn macro_record(ui: &egui::Ui, e: &mut MacroEdit, room: usize) {
    // Keep Space / Enter from pressing a focused button while recording.
    ui.memory_mut(|m| {
        if let Some(id) = m.focused() {
            m.surrender_focus(id);
        }
    });
    let (events, mods) = ui.input(|i| (i.events.clone(), i.modifiers));
    let mod_keys = [(mods.ctrl, e.mods.ctrl, 224u8), (mods.shift, e.mods.shift, 225), (mods.alt, e.mods.alt, 226)];
    let mut new: Vec<(u8, bool)> = mod_keys.iter().filter(|(now, was, _)| *now && !*was).map(|(_, _, c)| (*c, true)).collect();
    for ev in &events {
        if let egui::Event::Key { key, pressed, repeat: false, .. } = ev {
            if let Some(code) = hid_of(*key) {
                new.push((code, *pressed));
            }
        }
    }
    new.extend(mod_keys.iter().filter(|(now, was, _)| !*now && *was).map(|(_, _, c)| (*c, false)));
    e.mods = mods;
    for (code, press) in new {
        if e.events.len() >= room {
            e.recording = false;
            break;
        }
        let now = Instant::now();
        if let Some(prev) = e.events.last_mut() {
            let ms = e.last_event.map_or(u128::from(MACRO_DELAY_MS), |t| now.duration_since(t).as_millis());
            prev.delay_ms = if e.timed { ms.clamp(1, u128::from(u16::MAX)) as u16 } else { MACRO_DELAY_MS };
        }
        e.last_event = Some(now);
        e.events.push(MacroEvent { key: MacroKey::Key(code), press, delay_ms: 0 });
    }
}

/// Macro editor. Returns `Some(true)` on Apply, `Some(false)` on Cancel.
fn macro_editor(ui: &mut egui::Ui, e: &mut MacroEdit, room: usize) -> Option<bool> {
    ui.label(format!("Macro M{}", e.id + 1));
    ui.horizontal(|ui| {
        if ui.button(if e.recording { "Stop recording" } else { "Record" }).clicked() {
            e.recording = !e.recording;
            e.last_event = None;
            e.mods = ui.input(|i| i.modifiers);
        }
        ui.checkbox(&mut e.timed, "Keep my timing");
        ui.add_enabled_ui(!e.recording, |ui| {
            if ui.add_enabled(e.events.len() + 2 <= room, egui::Button::new("Add key")).clicked() {
                if let Some(last) = e.events.last_mut() {
                    last.delay_ms = last.delay_ms.max(MACRO_DELAY_MS);
                }
                e.events.push(MacroEvent { key: MacroKey::Key(4), press: true, delay_ms: MACRO_DELAY_MS });
                e.events.push(MacroEvent { key: MacroKey::Key(4), press: false, delay_ms: 0 });
            }
            if ui.button("Clear").clicked() {
                e.events.clear();
            }
        });
    });
    if e.recording {
        macro_record(ui, e, room);
        theme::note(ui, "Recording — type the keys now. Win and the right-hand Ctrl / Shift / Alt can't be told apart here; change them in the list afterwards.");
    } else {
        theme::help(ui, "Record your typing, or build the list by hand. Without Keep my timing every step waits 50 ms.");
    }
    let mut remove = None;
    let count = e.events.len();
    egui::Grid::new("macro_steps").num_columns(5).spacing([10.0, 4.0]).show(ui, |ui| {
        for (i, ev) in e.events.iter_mut().enumerate() {
            ui.label(theme::hint(&format!("{}", i + 1)));
            egui::ComboBox::from_id_salt(("macro_dir", i)).width(84.0).selected_text(if ev.press { "Press" } else { "Release" }).show_ui(ui, |ui| {
                ui.selectable_value(&mut ev.press, true, "Press");
                ui.selectable_value(&mut ev.press, false, "Release");
            });
            egui::ComboBox::from_id_salt(("macro_key", i)).width(120.0).height(320.0).selected_text(macro_key_label(ev.key)).show_ui(ui, |ui| {
                for (cat, keys) in macro_keys() {
                    ui.label(egui::RichText::new(cat).color(pal().muted));
                    for (label, code) in keys {
                        ui.selectable_value(&mut ev.key, MacroKey::Key(code), label);
                    }
                }
            });
            if i + 1 < count {
                ui.add(egui::DragValue::new(&mut ev.delay_ms).range(0..=u16::MAX).speed(1.0).suffix(" ms"));
            } else {
                ui.label(theme::hint("end"));
            }
            if ui.small_button("Remove").clicked() {
                remove = Some(i);
            }
            ui.end_row();
        }
    });
    if let Some(i) = remove {
        e.events.remove(i);
    }
    // Every press needs its release, or the key stays down after the macro.
    let mut down = std::collections::BTreeMap::new();
    for ev in &e.events {
        *down.entry(macro_key_label(ev.key)).or_insert(0i32) += if ev.press { 1 } else { -1 };
    }
    let stuck: Vec<&str> = down.iter().filter(|(_, n)| **n != 0).map(|(k, _)| k.as_str()).collect();
    if !stuck.is_empty() {
        theme::note(ui, format!("Press and Release don't pair up for: {}", stuck.join(", ")));
    }
    ui.label(theme::hint(&format!("{} steps (room for {room})", e.events.len())));
    let mut out = None;
    ui.horizontal(|ui| {
        let ok = !e.recording && !e.events.is_empty() && e.events.len() <= room && stuck.is_empty();
        if ui.add_enabled(ok, egui::Button::new("Apply")).clicked() {
            out = Some(true);
        }
        if ui.button("Cancel").clicked() {
            out = Some(false);
        }
    });
    out
}

/// What a span of `len` stages starting at `stage` does, e.g. "Press to Release".
fn dks_span_name(stage: usize, len: u8) -> String {
    match len {
        0 => "off".into(),
        1 => "Tap".into(),
        n => format!("Hold to {}", DKS_STAGES[(stage + n as usize - 1).min(3)]),
    }
}

/// One line for the Advanced keys list: `A (Press), B (Bottom to Top)`.
fn dks_summary(d: &Dks) -> String {
    let parts: Vec<String> = d
        .binds
        .iter()
        .filter(|b| b.action.kind != 0)
        .map(|b| {
            let spans: Vec<String> = (0..4)
                .filter(|&s| b.spans[s] > 0)
                .map(|s| match b.spans[s] {
                    1 => DKS_STAGES[s].to_string(),
                    n => format!("{} to {}", DKS_STAGES[s], DKS_STAGES[(s + n as usize - 1).min(3)]),
                })
                .collect();
            let when = if spans.is_empty() { "never".to_string() } else { spans.join(", ") };
            format!("{} ({when})", gen2::label(&b.action))
        })
        .collect();
    if parts.is_empty() {
        "no actions".into()
    } else {
        parts.join(" · ")
    }
}

/// DKS editor: the four stage depths, then one row per action with what it
/// does at each stage. Choosing a longer hold clears the stages it passes.
fn dks_editor(ui: &mut egui::Ui, dks: &mut Dks) {
    ui.horizontal(|ui| {
        for (i, name) in DKS_STAGES.iter().enumerate() {
            let (lo, hi) = gen2::dks_point_range(&dks.points, i);
            let mut mm = f32::from(dks.points[i]) / 10.0;
            ui.label(*name);
            let drag = egui::DragValue::new(&mut mm).range(f32::from(lo) / 10.0..=f32::from(hi) / 10.0).speed(0.02).fixed_decimals(1).suffix(" mm");
            if ui.add(drag).changed() {
                dks.points[i] = ((mm * 10.0).round() as u8).clamp(lo, hi);
            }
            ui.add_space(6.0);
        }
    });
    theme::help(ui, "Depth of each stage: Press and Bottom on the way down, Release and Top on the way back up.");
    egui::Grid::new("dks_binds").num_columns(6).spacing([10.0, 6.0]).show(ui, |ui| {
        ui.label("");
        for name in DKS_STAGES {
            ui.label(theme::hint(name));
        }
        ui.end_row();
        for (k, bind) in dks.binds.iter_mut().enumerate() {
            action_pick(ui, ("dks_action", k), &mut bind.action);
            let active = bind.action.kind != 0;
            for stage in 0..4 {
                // Inside an earlier, longer hold: nothing can start here.
                let held = (0..stage).any(|p| p + bind.spans[p] as usize > stage + 1);
                if held {
                    ui.label(theme::hint("held"));
                    continue;
                }
                let mut len = bind.spans[stage];
                ui.add_enabled_ui(active, |ui| {
                    egui::ComboBox::from_id_salt(("dks_span", k, stage)).width(118.0).selected_text(dks_span_name(stage, len)).show_ui(ui, |ui| {
                        for n in 0..=gen2::DKS_SPAN_MAX[stage] {
                            ui.selectable_value(&mut len, n, dks_span_name(stage, n));
                        }
                    });
                });
                if len != bind.spans[stage] {
                    bind.spans[stage] = len;
                    let end = (stage + len as usize).saturating_sub(1).min(4);
                    bind.spans.iter_mut().take(end).skip(stage + 1).for_each(|s| *s = 0);
                    bind.spans = gen2::dks_normalize(bind.spans);
                }
            }
            if active && ui.small_button("Clear").clicked() {
                *bind = gen2::DksBind::default();
            }
            ui.end_row();
        }
    });
    theme::help(ui, "Tap presses and lets go at that stage. Hold keeps the action down until the stage named.");
}

/// Draws `text` shrunk (down to 8 px, then wrapped) to fit `max_w`.
/// Takes the keyboard's copy `new` into the working copy, keeping elements the
/// user changed since the last sync (`work[i] != old[i]`), so a write that lands
/// mid-edit doesn't wipe newer or unrelated pending edits.
fn merge_pending<T: Clone + PartialEq>(work: &mut Vec<T>, old: &[T], new: &[T]) {
    if work.len() != old.len() || old.len() != new.len() {
        *work = new.to_vec();
        return;
    }
    for ((w, o), n) in work.iter_mut().zip(old).zip(new) {
        if *w == *o {
            *w = n.clone();
        }
    }
}

/// Draws `text` shrunk (down to 8 px) or wrapped to fit `max_w`; returns where it went.
fn fit_text(painter: &egui::Painter, anchor: egui::Pos2, align: Align2, text: &str, max_w: f32, size: f32, color: Color32) -> Rect {
    let mut s = size;
    let mut galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(s), color);
    while galley.size().x > max_w && s > 8.0 {
        s -= 1.0;
        galley = painter.layout_no_wrap(text.to_owned(), FontId::proportional(s), color);
    }
    if galley.size().x > max_w {
        galley = painter.layout(text.to_owned(), FontId::proportional(8.0), color, max_w.max(8.0));
    }
    let rect = align.anchor_size(anchor, galley.size());
    painter.galley(rect.min, galley, color);
    rect
}

fn ap_color(ap: f32) -> Color32 {
    theme::travel_color(ap)
}
