//! Gen-2 DrunkDeer boards (VID 0x19F5), e.g. the 2025 A75 Ultra (PID 0xFB3E).
//!
//! Spec source: the official web driver at drunkdeer.keybord.net.cn
//! (`app/page-feab963f2440235f.js`, class `j` = transport API, the class
//! after it = field encoding). See `docs/protocol-gen2.md`. Data tables
//! (key names, K75 layout) are in `data/gen2-a75ultra.json`.
//!
//! Wire format: report ID 0, 64-byte payload. Requests are
//! `55 cmd 00 sum len addr_lo addr_hi [last data..]`; replies start `AA`
//! with data from byte 8. One request in flight.

use std::sync::OnceLock;

use serde::Deserialize;

pub const VID: u16 = 0x19F5;
pub const REPORT_ID: u8 = 0;
pub const PAYLOAD_LEN: usize = 64;
/// Max data bytes per request.
pub const CHUNK: usize = 56;
/// Reply data offset.
pub const DATA: usize = 8;
pub const GROUP: u8 = 0x55;
pub const REPLY_OK: u8 = 0xAA;

pub type Packet = [u8; PAYLOAD_LEN];

/// Command bytes (second payload byte). Only these are ever sent.
/// Deliberately absent: 0xEE factory reset, 0x5F boot mode, 0x80–0x84
/// firmware, 0xA8/0xA9 calibration, group-6 `0F FF` factory settings.
pub mod cmd {
    pub const INFO: u8 = 0x03;
    pub const BASE: u8 = 0x04;
    pub const FUNC_READ: u8 = 0x05;
    pub const FUNC_WRITE: u8 = 0x06;
    pub const KEYMAP_DEFAULT: u8 = 0x07;
    pub const KEYMAP_READ: u8 = 0x08;
    pub const KEYMAP_WRITE: u8 = 0x09;
    pub const PROFILE_SELECT: u8 = 0x0E;
    pub const MACRO_READ: u8 = 0x0C;
    pub const MACRO_WRITE: u8 = 0x0D;
    /// Per-key colours (effect "Custom"), 128 × RGB at 512·profile.
    pub const COLORS_READ: u8 = 0x0A;
    pub const COLORS_WRITE: u8 = 0x0B;
    /// Advanced-key tables (per profile): DKS 768 B, MT/RS/SOCD/OKS 256 B, TGL 128 B.
    pub const DKS_READ: u8 = 0xA2;
    pub const DKS_WRITE: u8 = 0xA3;
    pub const MT_READ: u8 = 0xA4;
    pub const MT_WRITE: u8 = 0xA5;
    pub const TGL_READ: u8 = 0xA6;
    pub const TGL_WRITE: u8 = 0xA7;
    pub const TRIGGER_READ: u8 = 0xA0;
    pub const TRIGGER_WRITE: u8 = 0xA1;
    pub const PUBLIC_INFO: u8 = 0xB2;
}

/// First byte of unsolicited input reports.
pub mod notify {
    pub const CALIBRATION: u8 = 0xA0;
    /// `A1 ?? profile`: profile switched on the keyboard.
    pub const PROFILE: u8 = 0xA1;
    pub const RESET: u8 = 0xA2;
}

/// Known wired gen-2 boards. Only boards with a layout here are supported.
pub struct Board {
    pub pid: u16,
    pub name: &'static str,
    /// Official device type (`ea[..].type`).
    pub kind: u8,
}

pub const BOARDS: &[Board] = &[Board { pid: 0xFB3E, name: "A75 Ultra (2025)", kind: 101 }];

pub fn board(pid: u16) -> Option<&'static Board> {
    BOARDS.iter().find(|b| b.pid == pid)
}

/// Onboard profiles (`getBase()[0]`, official clamps > 3 to 0).
pub const PROFILES: u8 = 4;
/// Keymap layers: Main, FN1, FN2, FN3.
pub const LAYERS: usize = 4;
pub const KEYS: usize = 128;
pub const TRIGGER_BYTES: usize = 8;
pub const FUNC_LEN: usize = 64;

pub fn trigger_base(profile: u8) -> u16 {
    1024 * profile as u16
}

pub fn keymap_base(profile: u8, layer: usize) -> u16 {
    2048 * profile as u16 + 512 * layer as u16
}

pub fn func_base(profile: u8) -> u16 {
    64 * profile as u16
}

pub fn colors_base(profile: u8) -> u16 {
    512 * profile as u16
}

pub const MT_LEN: usize = 256;
pub const TGL_LEN: usize = 128;
pub const DKS_LEN: usize = 768;
/// Entries the official driver uses per table.
pub const ADV_ENTRIES: usize = 32;

pub fn mt_base(profile: u8) -> u16 {
    256 * profile as u16
}

pub fn tgl_base(profile: u8) -> u16 {
    128 * profile as u16
}

pub fn dks_base(profile: u8) -> u16 {
    768 * profile as u16
}

/// Knob actions are ordinary matrix slots; the factory map binds them to
/// Play/Pause (press), Vol- (left) and Vol+ (right) on type-101 boards.
pub fn knob_slots(factory_layer0: &[Action]) -> [Option<u8>; 3] {
    let find = |c: u8| factory_layer0.iter().position(|a| *a == Action { kind: kind::MEDIA, code1: c, code2: 0 }).map(|i| i as u8);
    [find(205), find(234), find(233)]
}

/// Official profile names (index 0..3).
pub const PROFILE_NAMES: [&str; 4] = ["Default", "Game", "Office", "Custom"];

fn sum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |a, b| a.wrapping_add(*b))
}

/// Read request: `55 cmd 00 sum len lo hi`, sum = len + lo + hi.
pub fn read(cmd: u8, addr: u16, len: u8) -> Packet {
    let [lo, hi] = addr.to_le_bytes();
    let mut p = [0u8; PAYLOAD_LEN];
    p[..7].copy_from_slice(&[GROUP, cmd, 0, sum(&[len, lo, hi]), len, lo, hi]);
    p
}

/// Write request: `55 cmd 00 sum len lo hi last data..`, sum over everything after it.
pub fn write(cmd: u8, addr: u16, data: &[u8], last: bool) -> Packet {
    assert!(data.len() <= CHUNK);
    let [lo, hi] = addr.to_le_bytes();
    let head = [data.len() as u8, lo, hi, last as u8];
    let mut p = [0u8; PAYLOAD_LEN];
    p[..3].copy_from_slice(&[GROUP, cmd, 0]);
    p[3] = sum(&head).wrapping_add(sum(data));
    p[4..8].copy_from_slice(&head);
    p[8..8 + data.len()].copy_from_slice(data);
    p
}

/// Reads of `len` bytes from `base`, split into ≤56-byte requests.
pub fn read_region(cmd: u8, base: u16, len: usize) -> Vec<Packet> {
    (0..len).step_by(CHUNK).map(|o| read(cmd, base + o as u16, (len - o).min(CHUNK) as u8)).collect()
}

/// Writes of `data` at `base` in ≤56-byte chunks; the final chunk is flagged `last`.
pub fn write_region(cmd: u8, base: u16, data: &[u8]) -> Vec<Packet> {
    let chunks: Vec<_> = data.chunks(CHUNK).collect();
    chunks
        .iter()
        .enumerate()
        .map(|(i, c)| write(cmd, base + (i * CHUNK) as u16, c, i + 1 == chunks.len()))
        .collect()
}

pub fn info() -> Packet {
    read(cmd::INFO, 0, 32)
}

pub fn base() -> Packet {
    read(cmd::BASE, 0, 32)
}

pub fn public_info() -> Packet {
    read(cmd::PUBLIC_INFO, 0, 9)
}

/// `setDeviceProfileData`: one byte (profile index) at address 0.
pub fn select_profile(index: u8) -> Packet {
    write(cmd::PROFILE_SELECT, 0, &[index], true)
}

/// Firmware version as the official driver shows it: hex of the LE u16 (e.g. `121`).
pub fn parse_fw(reply: &[u8]) -> Option<String> {
    let d = reply.get(DATA..DATA + 2)?;
    Some(format!("{:x}", u16::from_le_bytes([d[0], d[1]])))
}

/// 4-character device type from the public-info reply (e.g. `AP1`), if any.
pub fn parse_public_type(reply: &[u8]) -> Option<String> {
    if reply.len() < 17 || reply[0] != REPLY_OK {
        return None;
    }
    let s: String = reply[8..12].iter().filter(|&&b| b.is_ascii_graphic()).map(|&b| b as char).collect();
    let s = s.trim().to_string();
    (!s.is_empty()).then_some(s)
}

// ---------------------------------------------------------------- triggers

/// Rapid-trigger mode (`key_mode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RtMode {
    Off = 0,
    On = 1,
    /// "Continuous rapid trigger".
    Continuous = 2,
}

impl RtMode {
    pub const ALL: [RtMode; 3] = [RtMode::Off, RtMode::On, RtMode::Continuous];

    pub fn name(self) -> &'static str {
        match self {
            RtMode::Off => "Off",
            RtMode::On => "Rapid Trigger",
            RtMode::Continuous => "Continuous RT",
        }
    }
}

/// One key's 8-byte trigger record. Kept raw so fields we don't edit
/// (switch type, priority, precision, unknown bits) survive a write.
///
/// `[0]` 0xA0 | switch type · `[1]` priority<<4 | mode ·
/// `[2..3]` actuation-1 (9 bits; `[3]` bits 3-4 press precision, 1-2 release precision) ·
/// `[4..5]` press-1 (9 bits; `[5]>>1` top dead zone) · `[6..7]` release-1 (9 bits; `[7]>>1` bottom dead zone).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Trigger(pub [u8; 8]);

/// Value units for a precision setting: 0.01 / 0.005 / 0.001 mm.
pub fn units_per_mm(precision: u8) -> f32 {
    match precision {
        1 => 200.0,
        2 => 1000.0,
        _ => 100.0,
    }
}

impl Trigger {
    fn nine(lo: u8, hi: u8) -> u16 {
        (lo as u16 | ((hi as u16 & 1) << 8)) + 1
    }

    fn set_nine(lo: &mut u8, hi: &mut u8, units: u16) {
        let v = units.clamp(1, 512) - 1;
        *lo = v as u8;
        *hi = (*hi & !1) | (v >> 8) as u8;
    }

    pub fn switch_type(&self) -> u8 {
        self.0[0] & 0x0F
    }

    pub fn mode(&self) -> RtMode {
        match self.0[1] & 0x0F {
            0 => RtMode::Off,
            2 => RtMode::Continuous,
            _ => RtMode::On,
        }
    }

    pub fn set_mode(&mut self, m: RtMode) {
        self.0[1] = (self.0[1] & 0xF0) | m as u8;
    }

    pub fn press_precision(&self) -> u8 {
        (self.0[3] >> 3) & 3
    }

    pub fn release_precision(&self) -> u8 {
        (self.0[3] >> 1) & 3
    }

    /// Actuation point, mm (0.01 mm units).
    pub fn actuation_mm(&self) -> f32 {
        Self::nine(self.0[2], self.0[3]) as f32 / 100.0
    }

    pub fn set_actuation_mm(&mut self, mm: f32) {
        let [_, _, a, b, ..] = &mut self.0;
        Self::set_nine(a, b, (mm * 100.0).round() as u16);
    }

    pub fn press_mm(&self) -> f32 {
        Self::nine(self.0[4], self.0[5]) as f32 / units_per_mm(self.press_precision())
    }

    pub fn set_press_mm(&mut self, mm: f32) {
        let u = (mm * units_per_mm(self.press_precision())).round() as u16;
        let [_, _, _, _, a, b, ..] = &mut self.0;
        Self::set_nine(a, b, u);
    }

    pub fn release_mm(&self) -> f32 {
        Self::nine(self.0[6], self.0[7]) as f32 / units_per_mm(self.release_precision())
    }

    pub fn set_release_mm(&mut self, mm: f32) {
        let u = (mm * units_per_mm(self.release_precision())).round() as u16;
        let [_, _, _, _, _, _, a, b] = &mut self.0;
        Self::set_nine(a, b, u);
    }

    /// Top dead zone, mm (7 bits, 0.01 mm).
    pub fn top_dz_mm(&self) -> f32 {
        (self.0[5] >> 1) as f32 / 100.0
    }

    pub fn set_top_dz_mm(&mut self, mm: f32) {
        let v = ((mm * 100.0).round() as u8).min(127);
        self.0[5] = (self.0[5] & 1) | (v << 1);
    }

    /// Bottom dead zone, mm.
    pub fn bottom_dz_mm(&self) -> f32 {
        (self.0[7] >> 1) as f32 / 100.0
    }

    pub fn set_bottom_dz_mm(&mut self, mm: f32) {
        let v = ((mm * 100.0).round() as u8).min(127);
        self.0[7] = (self.0[7] & 1) | (v << 1);
    }
}

/// Switch types offered for type-101 boards (`tG`): name, full travel (0.01 mm).
pub const SWITCHES: &[(&str, u16)] = &[("Qian", 400), ("BeiDou", 360)];

/// Limits for type 101 (official: max 400, min 9, dead zone max 50, RT 1..400; 0.01 mm).
pub const ACTUATION_MM: (f32, f32) = (0.1, 4.0);
pub const RT_MM: (f32, f32) = (0.01, 4.0);
pub const DEADZONE_MM: (f32, f32) = (0.0, 0.5);

/// Max actuation for a key: switch travel minus bottom dead zone.
pub fn max_actuation_mm(t: &Trigger) -> f32 {
    let travel = SWITCHES.get(t.switch_type() as usize).map_or(400, |s| s.1) as f32 / 100.0;
    (travel - t.bottom_dz_mm()).min(ACTUATION_MM.1)
}

pub fn parse_triggers(data: &[u8]) -> Vec<Trigger> {
    data.as_chunks::<TRIGGER_BYTES>().0.iter().take(KEYS).map(|c| Trigger(*c)).collect()
}

pub fn encode_triggers(t: &[Trigger]) -> Vec<u8> {
    t.iter().flat_map(|t| t.0).collect()
}

// ---------------------------------------------------------------- keymap

/// Key action `[type, code1, code2]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Action {
    pub kind: u8,
    pub code1: u8,
    pub code2: u8,
}

pub mod kind {
    /// Keyboard: code1 = modifier mask, code2 = HID usage.
    pub const KEY: u8 = 16;
    pub const MOUSE: u8 = 32;
    pub const WHEEL: u8 = 33;
    /// Consumer usage, code1 | code2 << 8.
    pub const MEDIA: u8 = 48;
    pub const SYSTEM: u8 = 64;
    pub const MACRO: u8 = 112;
    pub const DKS: u8 = 144;
    pub const TGL: u8 = 145;
    pub const MT: u8 = 146;
    pub const RS: u8 = 147;
    pub const SOCD: u8 = 148;
    pub const OKS: u8 = 149;
    /// Keyboard functions: code1 = function, code2 = argument (FN layers: 255, n).
    pub const FUNCTION: u8 = 240;
}

impl Action {
    pub fn from_bytes(b: &[u8]) -> Self {
        Action { kind: b[0], code1: b[1], code2: b[2] }
    }

    /// HID code used to match layout keys (official `K()`): plain keys,
    /// modifiers as 224..231, FN1 = 255, FN2 = 254.
    pub fn match_code(&self) -> u16 {
        match (self.kind, self.code1, self.code2) {
            (kind::KEY, 0, c) => c as u16,
            (kind::KEY, m, _) if m.is_power_of_two() => 224 + m.trailing_zeros() as u16,
            (kind::FUNCTION, 255, 1) => 255,
            (kind::FUNCTION, 255, 2) | (224, 0, _) => 254,
            _ => 0,
        }
    }
}

pub fn parse_keymap(data: &[u8]) -> Vec<Action> {
    data.as_chunks::<3>().0.iter().take(KEYS).map(|c| Action::from_bytes(c.as_slice())).collect()
}

/// 128 × 3 bytes. Slots past the matrix hold filler (`F0+layer` ×3); they are
/// written back exactly as read.
pub fn encode_keymap(actions: &[Action]) -> Vec<u8> {
    actions.iter().flat_map(|a| [a.kind, a.code1, a.code2]).collect()
}

pub fn parse_colors(data: &[u8]) -> Vec<[u8; 3]> {
    data.as_chunks::<3>().0.iter().take(KEYS).copied().collect()
}

pub fn encode_colors(colors: &[[u8; 3]]) -> Vec<u8> {
    colors.iter().flatten().copied().collect()
}

// ---------------------------------------------------------------- advanced keys
//
// A key becomes "advanced" through its keymap entry; the entry points into a
// per-profile table (official `initAdvancedKeys` / `setAllMtKeyInfo`):
// - 146 MT  : `(146, k, hold_ms/10)`, MT table entry k = [tap action, hold action]
// - 147 RS / 148 SOCD / 149 OKS (pairs): key A `(t, k, slotB)`, key B `(t, k+1, slotA)`,
//   MT entries k = [A's action, B's action], k+1 = [B's action, A's action]
// - 145 TGL : `(145, k, 0)`, TGL table entry k = toggled action (3 B)
// - 144 DKS : `(144, k, 0)`, DKS table entry k (24 B: 4 points, 4 actions + stage bits)
// SOCD priority lives in the trigger record (`[1]` high nibble: 0 last input, 1 / 2 = this / other key).

/// Table entry `i` of a raw MT table: (first, second) = (tap, hold) for MT.
pub fn mt_entry(raw: &[u8], i: usize) -> (Action, Action) {
    let o = 6 * i;
    (Action::from_bytes(&raw[o..o + 3]), Action::from_bytes(&raw[o + 3..o + 6]))
}

pub fn set_mt_entry(raw: &mut [u8], i: usize, first: Action, second: Action) {
    let o = 6 * i;
    raw[o..o + 6].copy_from_slice(&[first.kind, first.code1, first.code2, second.kind, second.code1, second.code2]);
}

pub fn tgl_entry(raw: &[u8], i: usize) -> Action {
    Action::from_bytes(&raw[3 * i..3 * i + 3])
}

pub fn set_tgl_entry(raw: &mut [u8], i: usize, a: Action) {
    raw[3 * i..3 * i + 3].copy_from_slice(&[a.kind, a.code1, a.code2]);
}

/// DKS entry `i`: the four actions (display only).
pub fn dks_actions(raw: &[u8], i: usize) -> [Action; 4] {
    let o = DKS_ENTRY * i;
    [4, 9, 14, 19].map(|a| Action::from_bytes(&raw[o + a..o + a + 3]))
}

// ---------------------------------------------------------------- DKS (dynamic keystroke)
//
// One key, up to four actions, each pressed and released at four stages of the
// stroke: 0 press past point 0, 1 bottom (point 1), 2 release past point 2,
// 3 top (point 3). Entry (24 B, official `setDksKeyInfo` / `getDksKeyInfo`):
// points[4] in 0.1 mm, then 4 x (action 3 B + stage bits u16 LE).
// Stage bits: stage 0 = bits 0-2, stage 1 = bits 3-5, stage 2 = bits 6-8,
// stage 3 = bit 9; each field is a thermometer count (1 -> 001, 2 -> 011, 3 -> 111).

pub const DKS_ENTRY: usize = 24;
/// Official defaults: press 1.0 mm, bottom 3.0 mm, release 3.0 mm, top 1.0 mm.
pub const DKS_POINTS_DEFAULT: [u8; 4] = [10, 30, 30, 10];
/// Largest point value (3.0 mm).
pub const DKS_POINT_MAX: u8 = 30;
/// Longest span that can start at each stage.
pub const DKS_SPAN_MAX: [u8; 4] = [4, 3, 2, 1];

/// One DKS action. `spans[s]` = 0: nothing starts at stage `s`; 1: tap at `s`;
/// n > 1: pressed at `s` and held until stage `s + n - 1`. Stages inside a
/// longer span are 0 here (the keyboard's continuation bits are added on write).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DksBind {
    pub action: Action,
    pub spans: [u8; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dks {
    /// Stage points in 0.1 mm: press, bottom, release, top.
    pub points: [u8; 4],
    pub binds: [DksBind; 4],
}

impl Default for Dks {
    fn default() -> Self {
        Dks { points: DKS_POINTS_DEFAULT, binds: [DksBind::default(); 4] }
    }
}

impl Dks {
    /// The official driver refuses to save a DKS key without any action.
    pub fn is_empty(&self) -> bool {
        self.binds.iter().all(|b| b.action.kind == 0)
    }
}

fn thermometer(n: u8) -> u16 {
    u16::from(n > 0) | u16::from(n > 1) << 1 | u16::from(n > 2) << 2
}

/// Longest span the official editor allows at `stage`, given the other spans
/// (a span may end on a stage where another starts, but not pass it).
pub fn dks_span_limit(spans: &[u8; 4], stage: usize) -> u8 {
    match stage {
        0 if spans[1] > 0 => 2,
        0 if spans[2] > 0 => 3,
        1 if spans[2] > 0 => 2,
        s => DKS_SPAN_MAX[s.min(3)],
    }
}

/// Spans with every value inside the editor's limits and nothing under a longer span.
pub fn dks_normalize(mut spans: [u8; 4]) -> [u8; 4] {
    spans[3] = spans[3].min(1);
    spans[2] = spans[2].min(2);
    spans[1] = spans[1].min(dks_span_limit(&spans, 1));
    spans[0] = spans[0].min(dks_span_limit(&spans, 0));
    spans
}

/// Stage bits of one action, as the official driver writes them: stages inside
/// a 3- or 4-stage span carry the remaining length.
pub fn dks_stage_bits(spans: [u8; 4]) -> u16 {
    let [ds, mut de, mut us, ue] = dks_normalize(spans);
    let ds = match ds {
        4 => {
            (de, us) = (3, 2);
            3
        }
        3 => {
            de = 2;
            3
        }
        n => {
            if de == 3 {
                us = 2;
            }
            n
        }
    };
    thermometer(ds) | thermometer(de) << 3 | thermometer(us) << 6 | thermometer(ue.min(1)) << 9
}

/// Inverse of [`dks_stage_bits`] (official reader, continuation stages cleared).
pub fn dks_spans(bits: u16) -> [u8; 4] {
    let top = |v: u16| -> u8 {
        match v & 7 {
            v if v & 4 != 0 => 3,
            v if v & 2 != 0 => 2,
            v => v as u8,
        }
    };
    let (mut ds, mut de, mut us, ue) = (top(bits), top(bits >> 3), top(bits >> 6 & 3), (bits >> 9 & 1) as u8);
    if ds == 3 && de == 3 {
        if us == 0 {
            de = 0;
        } else {
            (ds, de, us) = (4, 0, 0);
        }
    }
    if ds == 3 {
        de = 0;
    }
    if de == 3 {
        us = 0;
    }
    [ds, de, us, ue]
}

/// Value range of point `i` (0.1 mm): press <= bottom, top <= release.
pub fn dks_point_range(points: &[u8; 4], i: usize) -> (u8, u8) {
    let or = |v: u8, d: u8| if v == 0 { d } else { v };
    match i {
        0 => (1, or(points[1], DKS_POINT_MAX).min(DKS_POINT_MAX)),
        1 => (or(points[0], 10).min(DKS_POINT_MAX), DKS_POINT_MAX),
        2 => (or(points[3], 10).min(DKS_POINT_MAX), DKS_POINT_MAX),
        _ => (1, or(points[2], DKS_POINT_MAX).min(DKS_POINT_MAX)),
    }
}

/// DKS entry `i` of a raw table.
pub fn dks_entry(raw: &[u8], i: usize) -> Dks {
    let e = &raw[DKS_ENTRY * i..DKS_ENTRY * (i + 1)];
    let mut d = Dks { points: [e[0], e[1], e[2], e[3]], ..Dks::default() };
    for (k, b) in d.binds.iter_mut().enumerate() {
        let o = 4 + 5 * k;
        b.action = Action::from_bytes(&e[o..o + 3]);
        b.spans = dks_spans(u16::from_le_bytes([e[o + 3], e[o + 4]]));
    }
    d
}

/// Writes DKS entry `i`; zero points become the official defaults.
pub fn set_dks_entry(raw: &mut [u8], i: usize, d: &Dks) {
    let e = &mut raw[DKS_ENTRY * i..DKS_ENTRY * (i + 1)];
    for (k, p) in d.points.iter().enumerate() {
        e[k] = if *p == 0 { DKS_POINTS_DEFAULT[k] } else { (*p).min(DKS_POINT_MAX) };
    }
    for (k, b) in d.binds.iter().enumerate() {
        let o = 4 + 5 * k;
        e[o..o + 3].copy_from_slice(&[b.action.kind, b.action.code1, b.action.code2]);
        e[o + 3..o + 5].copy_from_slice(&dks_stage_bits(b.spans).to_le_bytes());
    }
}

/// Zeroes DKS entry `i` (what the official driver leaves for unused entries).
pub fn clear_dks_entry(raw: &mut [u8], i: usize) {
    raw[DKS_ENTRY * i..DKS_ENTRY * (i + 1)].fill(0);
}

pub fn dks_entries_used(layers: &[Vec<Action>]) -> Vec<bool> {
    let mut used = vec![false; ADV_ENTRIES];
    for a in layers.iter().flatten() {
        if a.kind == kind::DKS && (a.code1 as usize) < ADV_ENTRIES {
            used[a.code1 as usize] = true;
        }
    }
    used
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairKind {
    Rs,
    Socd,
    Oks,
}

impl PairKind {
    pub fn kind(self) -> u8 {
        match self {
            PairKind::Rs => kind::RS,
            PairKind::Socd => kind::SOCD,
            PairKind::Oks => kind::OKS,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            PairKind::Rs => "Rappy Snappy",
            PairKind::Socd => "SOCD",
            PairKind::Oks => "One Key Switch",
        }
    }
}

/// An advanced binding found in one keymap layer (slots are matrix slots).
#[derive(Clone, Debug, PartialEq)]
pub enum Advanced {
    Mt { slot: u8, entry: u8, tap: Action, hold: Action, hold_ms: u16 },
    Pair { kind: PairKind, a: u8, b: u8, entry: u8, action_a: Action, action_b: Action },
    Tgl { slot: u8, entry: u8, action: Action },
    Dks { slot: u8, entry: u8, actions: [Action; 4] },
}

impl Advanced {
    pub fn slots(&self) -> Vec<u8> {
        match self {
            Advanced::Mt { slot, .. } | Advanced::Tgl { slot, .. } | Advanced::Dks { slot, .. } => vec![*slot],
            Advanced::Pair { a, b, .. } => vec![*a, *b],
        }
    }
}

/// Advanced bindings in a layer, decoded like the official driver.
pub fn find_advanced(layer: &[Action], mt: &[u8], tgl: &[u8], dks: &[u8]) -> Vec<Advanced> {
    let mut out = Vec::new();
    for (slot, a) in layer.iter().enumerate() {
        let (slot, e) = (slot as u8, a.code1 as usize);
        match a.kind {
            kind::MT if e < ADV_ENTRIES => {
                let (tap, hold) = mt_entry(mt, e);
                out.push(Advanced::Mt { slot, entry: e as u8, tap, hold, hold_ms: a.code2 as u16 * 10 });
            }
            kind::TGL if e < ADV_ENTRIES => out.push(Advanced::Tgl { slot, entry: e as u8, action: tgl_entry(tgl, e) }),
            kind::DKS if e < ADV_ENTRIES => out.push(Advanced::Dks { slot, entry: e as u8, actions: dks_actions(dks, e) }),
            k @ (kind::RS | kind::SOCD | kind::OKS) if e + 1 < ADV_ENTRIES => {
                // Report each pair once, from its first key (entry k; partner holds k+1).
                let partner = a.code2 as usize;
                let first_of_pair = layer.get(partner).is_some_and(|p| p.kind == k && p.code1 as usize == e + 1);
                if first_of_pair {
                    let (action_a, action_b) = mt_entry(mt, e);
                    let kind = match k {
                        kind::RS => PairKind::Rs,
                        kind::SOCD => PairKind::Socd,
                        _ => PairKind::Oks,
                    };
                    out.push(Advanced::Pair { kind, a: slot, b: partner as u8, entry: e as u8, action_a, action_b });
                }
            }
            _ => {}
        }
    }
    out
}

/// MT-table entries referenced by any layer (pairs use two).
pub fn mt_entries_used(layers: &[Vec<Action>]) -> Vec<bool> {
    let mut used = vec![false; ADV_ENTRIES];
    for a in layers.iter().flatten() {
        let e = a.code1 as usize;
        match a.kind {
            kind::MT | kind::RS | kind::SOCD | kind::OKS if e < ADV_ENTRIES => used[e] = true,
            _ => {}
        }
    }
    used
}

pub fn tgl_entries_used(layers: &[Vec<Action>]) -> Vec<bool> {
    let mut used = vec![false; ADV_ENTRIES];
    for a in layers.iter().flatten() {
        if a.kind == kind::TGL && (a.code1 as usize) < ADV_ENTRIES {
            used[a.code1 as usize] = true;
        }
    }
    used
}

/// First free entry (or first of `n` consecutive free entries).
pub fn free_entries(used: &[bool], n: usize) -> Option<usize> {
    (0..=used.len().saturating_sub(n)).find(|&i| used[i..i + n].iter().all(|u| !u))
}

// ---------------------------------------------------------------- macros
//
// One 2048 B block per profile (official `setMacro` / `getMacro`): 64 B header
// = 32 x u16 LE offset of a macro's first event, then 4 B events
// `[delay lo, delay hi, flags, code]`. delay = ms to wait after the event;
// flags: bits 0-5 type (1 modifier mask, 2 key, 3 mouse), bit 6 press, bit 7
// last event of the macro. A key plays macro `id` through keymap `112 id 1`.

pub const MACRO_LEN: usize = 2048;
pub const MACRO_HEADER: usize = 64;
/// Header entries.
pub const MACRO_SLOTS: usize = 32;
/// Macros the official driver offers (M1..M16).
pub const MACROS: usize = 16;
/// Events that fit in a block, all macros together.
pub const MACRO_EVENTS_MAX: usize = (MACRO_LEN - MACRO_HEADER) / 4;

pub fn macro_base(profile: u8) -> u16 {
    2048 * profile as u16
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroKey {
    /// HID usage; 224..=231 are the modifiers.
    Key(u8),
    /// Mouse button as stored (the official reader knows it, its recorder never writes it).
    Mouse(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacroEvent {
    pub key: MacroKey,
    pub press: bool,
    /// Wait after this event, ms (written as 0 on a macro's last event).
    pub delay_ms: u16,
}

/// Keymap entry that plays macro `id` once (the official bind always writes 1).
pub fn macro_action(id: u8) -> Action {
    Action { kind: kind::MACRO, code1: id, code2: 1 }
}

/// Macro ids referenced by any layer.
pub fn macros_bound(layers: &[Vec<Action>]) -> Vec<bool> {
    let mut used = vec![false; MACRO_SLOTS];
    for a in layers.iter().flatten() {
        if a.kind == kind::MACRO && (a.code1 as usize) < MACRO_SLOTS {
            used[a.code1 as usize] = true;
        }
    }
    used
}

/// Block for `macros` (index = id), as the official driver writes it: header
/// plus events only. `None` if there are more than 32 macros or the events
/// don't fit. Empty macros get offset 64 like the first stored one.
pub fn encode_macros(macros: &[Vec<MacroEvent>]) -> Option<Vec<u8>> {
    let total: usize = macros.iter().map(Vec::len).sum();
    if macros.len() > MACRO_SLOTS || total > MACRO_EVENTS_MAX {
        return None;
    }
    let mut out = vec![0u8; MACRO_HEADER];
    for (id, events) in macros.iter().enumerate() {
        let offset = if events.is_empty() { MACRO_HEADER } else { out.len() };
        out[2 * id..2 * id + 2].copy_from_slice(&(offset as u16).to_le_bytes());
        for (i, e) in events.iter().enumerate() {
            let last = i + 1 == events.len();
            let (kind, code) = match e.key {
                MacroKey::Key(c @ 224..=231) => (1, 1 << (c & 15)),
                MacroKey::Key(c) => (2, c),
                MacroKey::Mouse(b) => (3, b),
            };
            let delay = if last { 0 } else { e.delay_ms };
            out.extend_from_slice(&delay.to_le_bytes());
            out.extend_from_slice(&[kind | (e.press as u8) << 6 | (last as u8) << 7, code]);
        }
    }
    Some(out)
}

/// The 32 macros of a block. Offset 64 means "empty" as well as "first stored
/// macro", so the events at 64 go to the lowest such id that is `bound` to a
/// key, else to the lowest such id.
pub fn parse_macros(raw: &[u8], bound: &[bool]) -> Vec<Vec<MacroEvent>> {
    let mut out = vec![Vec::new(); MACRO_SLOTS];
    if raw.len() < MACRO_HEADER {
        return out;
    }
    let offset = |id: usize| u16::from_le_bytes([raw[2 * id], raw[2 * id + 1]]) as usize;
    let at_start: Vec<usize> = (0..MACRO_SLOTS).filter(|&id| offset(id) == MACRO_HEADER).collect();
    let first = at_start.iter().copied().find(|&id| bound.get(id).copied().unwrap_or(false)).or(at_start.first().copied());
    for (id, events) in out.iter_mut().enumerate() {
        let mut o = offset(id);
        if o < MACRO_HEADER || (o == MACRO_HEADER && first != Some(id)) {
            continue;
        }
        while let Some(e) = raw.get(o..o + 4) {
            let (flags, code) = (e[2], e[3]);
            if code == 0 {
                break;
            }
            let key = match flags & 0x3F {
                1 => MacroKey::Key(224 + code.trailing_zeros() as u8),
                3 => MacroKey::Mouse(code),
                _ => MacroKey::Key(code),
            };
            events.push(MacroEvent { key, press: flags & 0x40 != 0, delay_ms: u16::from_le_bytes([e[0], e[1]]) });
            if flags & 0x80 != 0 {
                break;
            }
            o += 4;
        }
    }
    out
}

impl Trigger {
    /// SOCD priority nibble (`[1] >> 4`): 0 last input wins, 1 this key, 2 the other key.
    pub fn priority(&self) -> u8 {
        self.0[1] >> 4
    }

    pub fn set_priority(&mut self, p: u8) {
        self.0[1] = (self.0[1] & 0x0F) | (p << 4);
    }
}

/// One entry of the remap picker.
pub struct PickItem {
    pub label: String,
    pub action: Action,
}

/// Function codes never offered in the picker (keyboard-side boot/factory
/// reset, wireless and side-light keys this wired board doesn't have).
const PICKER_SKIP_FUNCTIONS: &[u8] = &[0, 7, 8, 16, 17, 18, 19, 21, 29, 31, 49, 64, 66, 88, 160];

/// Picker categories built from the official tables.
pub fn picker() -> &'static [(&'static str, Vec<PickItem>)] {
    static P: OnceLock<Vec<(&'static str, Vec<PickItem>)>> = OnceLock::new();
    P.get_or_init(|| {
        // Basic keys with a name in the official table, in code order.
        fn keys(codes: impl IntoIterator<Item = u8>) -> Vec<PickItem> {
            codes
                .into_iter()
                .filter_map(|c| basic_name(c).map(|label| PickItem { label, action: Action { kind: kind::KEY, code1: 0, code2: c } }))
                .collect()
        }
        let mut letters: Vec<PickItem> = keys(4..=29);
        letters.sort_by(|a, b| a.label.cmp(&b.label));
        let mods = ["LCtrl", "LShift", "LAlt", "LWin", "RCtrl", "RShift", "RAlt", "RWin"]
            .iter()
            .enumerate()
            .map(|(i, n)| PickItem { label: n.to_string(), action: Action { kind: kind::KEY, code1: 1 << i, code2: 0 } })
            .collect();
        let functions = data()
            .functions
            .iter()
            .filter(|f| match f.0 {
                255 => (1..=3).contains(&f.1),
                c => (!PICKER_SKIP_FUNCTIONS.contains(&c) && c < 250) || (250..=253).contains(&c),
            })
            .map(|f| PickItem { label: f.2.clone(), action: Action { kind: kind::FUNCTION, code1: f.0, code2: f.1 } })
            .collect();
        let media = data().media.iter().map(|m| PickItem { label: m.2.clone(), action: Action { kind: kind::MEDIA, code1: m.0, code2: m.1 } }).collect();
        let mouse = data().mouse.iter().map(|m| PickItem { label: m.3.clone(), action: Action { kind: m.0, code1: m.1, code2: m.2 } }).collect();
        vec![
            ("Letters", letters),
            ("Numbers & symbols", keys([30..=39, 45..=49, 51..=56].into_iter().flatten())),
            ("Editing & navigation", keys([40..=44, 57..=57, 70..=82, 101..=101].into_iter().flatten())),
            ("F-keys", keys([58..=69, 104..=115].into_iter().flatten())),
            ("Modifiers", mods),
            ("Layers & keyboard", functions),
            ("Numpad", keys(83..=99)),
            ("Media", media),
            ("Mouse", mouse),
        ]
    })
}

#[derive(Deserialize)]
struct Data {
    basic: Vec<(u16, String)>,
    functions: Vec<(u8, u8, String)>,
    media: Vec<(u8, u8, String)>,
    mouse: Vec<(u8, u8, u8, String)>,
    layout_k75: Vec<LayoutKey>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LayoutKey {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// HID code of the factory mapping (FN1 = 255, FN2 = 254).
    pub code: u16,
    pub name: String,
}

fn data() -> &'static Data {
    static D: OnceLock<Data> = OnceLock::new();
    D.get_or_init(|| serde_json::from_str(include_str!("../data/gen2-a75ultra.json")).expect("gen2 data"))
}

pub fn layout_k75() -> &'static [LayoutKey] {
    &data().layout_k75
}

/// Layout key -> matrix index, the way the official driver does it: each
/// layout key takes the first unused matrix slot whose factory layer-0
/// action has the same code. `None` = key not found in the factory map.
pub fn match_layout(layout: &[LayoutKey], factory_layer0: &[Action]) -> Vec<Option<u8>> {
    let mut used = vec![false; factory_layer0.len()];
    layout
        .iter()
        .map(|k| {
            let i = factory_layer0.iter().enumerate().position(|(i, a)| !used[i] && a.match_code() == k.code)?;
            used[i] = true;
            Some(i as u8)
        })
        .collect()
}

fn basic_name(code: u8) -> Option<String> {
    if code == 0 {
        return None;
    }
    data().basic.iter().find(|(c, _)| *c == code as u16).map(|(_, n)| short_name(n))
}

/// Official names are "shifted\nplain" for symbol keys and use arrow glyphs;
/// keep the plain symbol and spell arrows out (the UI font lacks them).
fn short_name(n: &str) -> String {
    let n = match n {
        "↑" => "Up",
        "↓" => "Down",
        "←" => "Left",
        "→" => "Right",
        other => other,
    };
    match n.split_once('\n') {
        Some((a, b)) if a.chars().count() <= 2 && b.chars().count() <= 2 => b.to_string(),
        Some((a, b)) => format!("{a} {b}"),
        None => n.to_string(),
    }
}

/// Display name (official `M()`).
pub fn label(a: &Action) -> String {
    match a.kind {
        kind::KEY => {
            let key = basic_name(a.code2).unwrap_or_default();
            if a.code1 == 0 {
                return key;
            }
            let m = a.code1;
            let mut parts: Vec<String> = Vec::new();
            for (bits, name) in [((m >> 2) & 1 | (m >> 6) & 1, "Alt"), ((m >> 1) & 1 | (m >> 5) & 1, "Shift"), (m & 1 | (m >> 4) & 1, "Ctrl"), ((m >> 3) & 1 | (m >> 7) & 1, "Win")] {
                if bits != 0 {
                    parts.push(name.to_string());
                }
            }
            if !key.is_empty() {
                parts.push(key);
            }
            parts.join("+")
        }
        kind::MACRO => format!("M{}", a.code1 as u16 + 1),
        kind::DKS => "DKS".into(),
        kind::TGL => "TGL".into(),
        kind::MT => "MT".into(),
        kind::RS => "RS".into(),
        kind::SOCD => "SOCD".into(),
        kind::OKS => "OKS".into(),
        kind::SYSTEM => ["", "Power", "Sleep", "", "WakeUp"].get(a.code1 as usize).unwrap_or(&"").to_string(),
        kind::MOUSE | kind::WHEEL => {
            data().mouse.iter().find(|m| m.0 == a.kind && m.1 == a.code1 && m.2 == a.code2).map(|m| m.3.clone()).unwrap_or_default()
        }
        kind::FUNCTION => data().functions.iter().find(|f| f.0 == a.code1 && f.1 == a.code2).map(|f| f.2.clone()).unwrap_or_default(),
        kind::MEDIA => data().media.iter().find(|f| f.0 == a.code1 && f.1 == a.code2).map(|f| f.2.clone()).unwrap_or_default(),
        _ => String::new(),
    }
}

// ---------------------------------------------------------------- func block

/// Per-profile settings block (64 bytes, `getFunc`). Field offsets from
/// `getBaseInfo` / the setters; everything is read-modify-write.
pub mod func {
    /// (byte, mask, shift)
    pub type Field = (usize, u8, u8);
    /// 2 = macOS.
    pub const SYSTEM_MODE: Field = (1, 0x0F, 0);
    /// 1 = 8000 Hz, 2 = 4000, 3 = 2000, 4 = 1000.
    pub const POLLING: Field = (4, 0x0F, 0);
    /// 0 none, 1 = 30, 2 = 60.
    pub const TICK: Field = (4, 0xF0, 4);
    pub const LOCK_WIN: Field = (6, 0x01, 0);
    pub const LOCK_ALT_TAB: Field = (6, 0x02, 1);
    pub const LOCK_ALT_F4: Field = (6, 0x04, 2);
    /// "Adaptive Stable" (`berserkMode`).
    pub const ADAPTIVE: Field = (7, 0x01, 0);
    /// 0 low, 1 medium, 2 high.
    pub const DEBOUNCE: Field = (7, 0xE0, 5);
    pub const LIGHT_EFFECT: Field = (8, 0xFF, 0);
    /// 0..100.
    pub const LIGHT_BRIGHTNESS: Field = (9, 0xFF, 0);
    /// Stored as 4 - speed (speed 0..4).
    pub const LIGHT_SPEED_INV: Field = (10, 0xFF, 0);
    pub const LIGHT_DIRECTION: Field = (11, 0xFF, 0);
    /// 0 = single colour (RGB at 14..16).
    pub const LIGHT_MULTI: Field = (12, 0xFF, 0);
    pub const LIGHT_R: Field = (14, 0xFF, 0);
    pub const LIGHT_G: Field = (15, 0xFF, 0);
    pub const LIGHT_B: Field = (16, 0xFF, 0);

    pub fn get(block: &[u8], f: Field) -> u8 {
        (block[f.0] & f.1) >> f.2
    }

    pub fn set(block: &mut [u8], f: Field, v: u8) {
        block[f.0] = (block[f.0] & !f.1) | ((v << f.2) & f.1);
    }

    pub const POLLING_RATES: &[(u8, &str)] = &[(1, "8000 Hz"), (2, "4000 Hz"), (3, "2000 Hz"), (4, "1000 Hz")];
    pub const TICK_RATES: &[(u8, &str)] = &[(0, "Off"), (1, "30"), (2, "60")];
    pub const DEBOUNCE_LEVELS: &[(u8, &str)] = &[(0, "Low"), (1, "Medium"), (2, "High")];
}

/// Lighting effects for the K75 layout (`lighting.effect`, value = index).
/// "Custom" is per-key colour (not editable here yet).
pub const EFFECTS: &[&str] = &[
    "Custom",
    "Spectrum",
    "Ladder",
    "Static",
    "Breathe",
    "Flowers",
    "Wave",
    "Up and down waves",
    "Fountain",
    "Galaxy",
    "Spin",
    "Tide",
    "Swing",
    "Ripples",
    "Steady on Ripple",
    "Single order",
    "Palace",
    "Piano",
    "Streamer",
    "Rain",
    "Starlight",
    "Fireworks",
    "Wave zone",
];

/// Effects without a colour choice (K75 `color: false`).
pub fn effect_has_color(effect: u8) -> bool {
    !matches!(effect, 0 | 1 | 5)
}

/// Effects with a speed setting (K75 `speed: true`).
pub fn effect_has_speed(effect: u8) -> bool {
    !matches!(effect, 0 | 3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_requests_match_official() {
        // getInfo: sendDeviceData(85, [3, 0, 32, 32])
        assert_eq!(&info()[..5], &[0x55, 0x03, 0x00, 0x20, 0x20]);
        assert_eq!(&base()[..5], &[0x55, 0x04, 0x00, 0x20, 0x20]);
        // getPublicDeviceInfo: [178, 0, 9, 9, 0, 0]
        assert_eq!(&public_info()[..7], &[0x55, 0xB2, 0, 9, 9, 0, 0]);
        // getKeyTrigger chunk 2 of profile 1: addr 1024 + 112, len 56, sum = (lo+hi+len)&255
        let p = read_region(cmd::TRIGGER_READ, trigger_base(1), 1024)[2];
        let addr = 1024u16 + 112;
        let [lo, hi] = addr.to_le_bytes();
        assert_eq!(&p[..7], &[0x55, 0xA0, 0, lo.wrapping_add(hi).wrapping_add(56), 56, lo, hi]);
        assert_eq!(read_region(cmd::TRIGGER_READ, 0, 1024).len(), 19);
        assert_eq!(read_region(cmd::TRIGGER_READ, 0, 1024)[18][4] as usize, 1024 - 18 * 56);
    }

    #[test]
    fn write_requests_match_official() {
        // setDeviceProfileData(2): [14, 0, sum([1,0,0,1,2,0]), 1,0,0,1,2,0]
        let p = select_profile(2);
        assert_eq!(&p[..10], &[0x55, 0x0E, 0, 4, 1, 0, 0, 1, 2, 0][..10]);
        let w = write_region(cmd::FUNC_WRITE, func_base(1), &[7u8; 64]);
        assert_eq!(w.len(), 2);
        assert_eq!(&w[0][4..8], &[56, 64, 0, 0]);
        assert_eq!(&w[1][4..8], &[8, 120, 0, 1]);
        assert_eq!(w[1][3], (8u8 + 120 + 1).wrapping_add(7 * 8));
    }

    #[test]
    fn trigger_fields_roundtrip() {
        // actuation 150 units (1.50 mm) -> stored 149 = 0x95; press 30 -> 29; release 400 -> 399 = 0x18F
        let mut t = Trigger([0xA1, 0x01, 0x95, 0x00, 29, 10 << 1, 0x8F, 1 | (5 << 1)]);
        assert_eq!(t.switch_type(), 1);
        assert_eq!(t.mode(), RtMode::On);
        assert!((t.actuation_mm() - 1.5).abs() < 1e-6);
        assert!((t.press_mm() - 0.3).abs() < 1e-6);
        assert!((t.release_mm() - 4.0).abs() < 1e-6);
        assert!((t.top_dz_mm() - 0.10).abs() < 1e-6);
        assert!((t.bottom_dz_mm() - 0.05).abs() < 1e-6);
        let before = t;
        t.set_actuation_mm(t.actuation_mm());
        t.set_press_mm(t.press_mm());
        t.set_release_mm(t.release_mm());
        t.set_top_dz_mm(t.top_dz_mm());
        t.set_bottom_dz_mm(t.bottom_dz_mm());
        assert_eq!(t, before, "no-op edits keep bytes");
        t.set_actuation_mm(3.3);
        assert_eq!((t.0[2], t.0[3] & 1), (0x49, 1)); // 330 - 1 = 329 = 0x149
        t.set_mode(RtMode::Continuous);
        assert_eq!(t.0[1], 0x02);
        t.set_top_dz_mm(0.5);
        assert_eq!(t.0[5] >> 1, 50);
        assert_eq!(t.0[5] & 1, 0, "press bit 8 kept");
    }

    #[test]
    fn layout_matching() {
        let layout = layout_k75();
        assert_eq!(layout.len(), 82);
        // Factory map in layout order with FN keys and modifiers encoded the device way.
        let mut factory: Vec<Action> = layout
            .iter()
            .map(|k| match k.code {
                255 => Action { kind: kind::FUNCTION, code1: 255, code2: 1 },
                254 => Action { kind: kind::FUNCTION, code1: 255, code2: 2 },
                c @ 224..=231 => Action { kind: kind::KEY, code1: 1 << (c - 224), code2: 0 },
                c => Action { kind: kind::KEY, code1: 0, code2: c as u8 },
            })
            .collect();
        factory.reverse();
        factory.resize(KEYS, Action::default());
        let idx = match_layout(layout, &factory);
        assert!(idx.iter().all(Option::is_some));
        assert_eq!(idx[0], Some(81)); // Esc is last in the reversed map
        let mut seen = idx.clone();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 82, "unique slots");
    }

    #[test]
    fn labels() {
        assert_eq!(label(&Action { kind: kind::KEY, code1: 0, code2: 4 }), "A");
        assert_eq!(label(&Action { kind: kind::KEY, code1: 0, code2: 82 }), "Up");
        assert_eq!(label(&Action { kind: kind::KEY, code1: 0, code2: 30 }), "1");
        assert_eq!(label(&Action { kind: kind::KEY, code1: 0, code2: 83 }), "Num Lock");
        assert_eq!(label(&Action { kind: kind::KEY, code1: 0x03, code2: 4 }), "Shift+Ctrl+A");
        assert_eq!(label(&Action { kind: kind::FUNCTION, code1: 255, code2: 1 }), "FN1");
        assert_eq!(label(&Action { kind: kind::MEDIA, code1: 233, code2: 0 }), "Vol+");
        assert_eq!(label(&Action { kind: kind::MACRO, code1: 0, code2: 0 }), "M1");
    }

    #[test]
    fn picker_is_safe_and_complete() {
        let all: Vec<&PickItem> = picker().iter().flat_map(|(_, v)| v).collect();
        assert!(all.len() > 120, "{}", all.len());
        assert!(all.iter().all(|i| !i.label.is_empty()));
        // Never offer keyboard-side boot / factory reset.
        assert!(!all.iter().any(|i| i.action.kind == kind::FUNCTION && matches!(i.action.code1, 7 | 8)));
        let fn1 = all.iter().find(|i| i.label == "FN1").unwrap();
        assert_eq!(fn1.action, Action { kind: kind::FUNCTION, code1: 255, code2: 1 });
        assert!(all.iter().any(|i| i.label == "LAlt" && i.action == Action { kind: kind::KEY, code1: 4, code2: 0 }));
        assert_eq!(label(&Action { kind: kind::KEY, code1: 4, code2: 0 }), "Alt");
        assert_eq!(parse_public_type(&[0xAA, 0xB2, 0, 0, 0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0, 0, 0, 0]), None);
    }

    #[test]
    fn advanced_from_user_log() {
        // User's layer 0 (fw 121): two SOCD pairs at slots 88..91, knob at 92..94.
        let mut layer = vec![Action::default(); KEYS];
        layer[88] = Action { kind: kind::SOCD, code1: 0, code2: 91 };
        layer[89] = Action { kind: kind::SOCD, code1: 3, code2: 90 };
        layer[90] = Action { kind: kind::SOCD, code1: 2, code2: 89 };
        layer[91] = Action { kind: kind::SOCD, code1: 1, code2: 88 };
        layer[92] = Action { kind: kind::MEDIA, code1: 205, code2: 0 };
        layer[93] = Action { kind: kind::MEDIA, code1: 233, code2: 0 };
        layer[94] = Action { kind: kind::MEDIA, code1: 234, code2: 0 };
        layer[10] = Action { kind: kind::MT, code1: 4, code2: 20 };
        let a = Action { kind: kind::KEY, code1: 0, code2: 4 };
        let d = Action { kind: kind::KEY, code1: 0, code2: 7 };
        let ctrl = Action { kind: kind::KEY, code1: 1, code2: 0 };
        let esc = Action { kind: kind::KEY, code1: 0, code2: 41 };
        let mut mt = vec![0u8; MT_LEN];
        set_mt_entry(&mut mt, 0, a, d);
        set_mt_entry(&mut mt, 1, d, a);
        set_mt_entry(&mut mt, 2, d, a);
        set_mt_entry(&mut mt, 3, a, d);
        set_mt_entry(&mut mt, 4, esc, ctrl);
        let adv = find_advanced(&layer, &mt, &[0; TGL_LEN], &[0; DKS_LEN]);
        assert_eq!(adv.len(), 3, "{adv:?}");
        assert!(adv.contains(&Advanced::Pair { kind: PairKind::Socd, a: 88, b: 91, entry: 0, action_a: a, action_b: d }));
        assert!(adv.contains(&Advanced::Pair { kind: PairKind::Socd, a: 90, b: 89, entry: 2, action_a: d, action_b: a }));
        assert!(adv.contains(&Advanced::Mt { slot: 10, entry: 4, tap: esc, hold: ctrl, hold_ms: 200 }));
        let used = mt_entries_used(&[layer.clone()]);
        assert_eq!(&used[..6], &[true, true, true, true, true, false]);
        assert_eq!(free_entries(&used, 2), Some(5));
        assert_eq!(knob_slots(&layer), [Some(92), Some(94), Some(93)]);
        let mut t = Trigger([0xA0, 0x01, 0, 0, 0, 0, 0, 0]);
        t.set_priority(2);
        assert_eq!((t.0[1], t.priority(), t.mode()), (0x21, 2, RtMode::On));
    }

    #[test]
    fn dks_stage_bits_match_official() {
        // Official writer: thermometer fields, continuation bits under long spans.
        for (spans, bits) in [
            ([0, 0, 0, 0], 0x000),
            ([1, 0, 0, 0], 0x001),
            ([2, 0, 0, 0], 0x003),
            ([3, 0, 0, 0], 0x01F), // 111 + stage 1 = 011
            ([4, 0, 0, 0], 0x0FF), // 111, 111, stage 2 = 011
            ([4, 0, 0, 1], 0x2FF),
            ([0, 3, 0, 0], 0x0F8), // stage 1 = 111, stage 2 = 011
            ([1, 1, 1, 1], 0x249),
            ([2, 2, 2, 1], 0x2DB),
            ([3, 0, 1, 0], 0x05F),
            ([3, 0, 2, 1], 0x2DF),
        ] {
            assert_eq!(dks_stage_bits(spans), bits, "{spans:?}");
            assert_eq!(dks_spans(bits), spans, "{bits:#x}");
        }
        // Spans the editor can't produce are cut back to its limits.
        assert_eq!(dks_normalize([4, 1, 0, 0]), [2, 1, 0, 0]);
        assert_eq!(dks_normalize([4, 0, 1, 0]), [3, 0, 1, 0]);
        assert_eq!(dks_normalize([0, 3, 2, 9]), [0, 2, 2, 1]);
        assert_eq!(dks_span_limit(&[0, 0, 0, 1], 0), 4);
        // Official reader quirk: 111/111 without stage-2 bits reads as a 3-stage span.
        assert_eq!(dks_spans(0x03F), [3, 0, 0, 0]);
    }

    #[test]
    fn dks_entry_golden() {
        let key = |c: u8| Action { kind: kind::KEY, code1: 0, code2: c };
        let mut d = Dks::default();
        assert!(d.is_empty());
        d.binds[0] = DksBind { action: key(4), spans: [1, 0, 0, 0] };
        d.binds[1] = DksBind { action: key(5), spans: [0, 1, 0, 0] };
        d.binds[3] = DksBind { action: key(6), spans: [4, 0, 0, 1] };
        let mut raw = vec![0u8; DKS_LEN];
        set_dks_entry(&mut raw, 1, &d);
        #[rustfmt::skip]
        let want = [
            10, 30, 30, 10,
            16, 0, 4, 0x01, 0x00,
            16, 0, 5, 0x08, 0x00,
            0, 0, 0, 0x00, 0x00,
            16, 0, 6, 0xFF, 0x02,
        ];
        assert_eq!(raw[24..48], want);
        assert!(raw[..24].iter().chain(&raw[48..]).all(|b| *b == 0));
        assert_eq!(dks_entry(&raw, 1), d);
        assert_eq!(dks_actions(&raw, 1), [key(4), key(5), Action::default(), key(6)]);
        // Zero points are written as the official defaults; values are capped at 3.0 mm.
        set_dks_entry(&mut raw, 0, &Dks { points: [0, 200, 0, 5], ..d });
        assert_eq!(raw[..4], [10, 30, 30, 5]);
        clear_dks_entry(&mut raw, 0);
        assert!(raw[..24].iter().all(|b| *b == 0));
        // Point ranges: press <= bottom, top <= release.
        assert_eq!(dks_point_range(&[10, 25, 30, 12], 0), (1, 25));
        assert_eq!(dks_point_range(&[10, 25, 30, 12], 1), (10, 30));
        assert_eq!(dks_point_range(&[10, 25, 28, 12], 2), (12, 30));
        assert_eq!(dks_point_range(&[10, 25, 28, 12], 3), (1, 28));
        // Table address and write framing (official `sendData(163, 768 * profile, ...)`).
        assert_eq!(dks_base(2), 1536);
        let pk = write_region(cmd::DKS_WRITE, dks_base(0), &raw);
        assert_eq!(pk.len(), 14);
        assert_eq!(pk[0][..2], [GROUP, 0xA3]);
        let layer = vec![Action { kind: kind::DKS, code1: 1, code2: 0 }];
        let used = dks_entries_used(&[layer]);
        assert_eq!(free_entries(&used, 1), Some(0));
        assert!(used[1]);
    }

    #[test]
    fn macro_block_golden() {
        let ev = |code: u8, press: bool, delay_ms: u16| MacroEvent { key: MacroKey::Key(code), press, delay_ms };
        // M1 empty, M2 = Ctrl+C (LCtrl 224, C 6), M3 = A tap with a 300 ms hold.
        let m2 = vec![ev(224, true, 50), ev(6, true, 50), ev(6, false, 50), ev(224, false, 77)];
        let m3 = vec![ev(4, true, 300), ev(4, false, 0)];
        let raw = encode_macros(&[vec![], m2.clone(), m3.clone()]).unwrap();
        #[rustfmt::skip]
        let events = [
            50, 0, 0x41, 0x01,   // LCtrl down (modifier mask), then 50 ms
            50, 0, 0x42, 6,      // C down
            50, 0, 0x02, 6,      // C up
            0, 0, 0x81, 0x01,    // LCtrl up, last: delay written as 0
            0x2C, 0x01, 0x42, 4, // A down, 300 ms
            0, 0, 0x82, 4,       // A up, last
        ];
        assert_eq!(raw.len(), 64 + events.len());
        assert_eq!(raw[..8], [64, 0, 64, 0, 80, 0, 0, 0]);
        assert!(raw[8..64].iter().all(|b| *b == 0));
        assert_eq!(raw[64..], events);

        let mut block = raw.clone();
        block.resize(MACRO_LEN, 0);
        // M1 and M2 both sit at offset 64: a key bound to M2 settles it.
        let mut bound = vec![false; MACRO_SLOTS];
        bound[1] = true;
        let back = parse_macros(&block, &bound);
        assert_eq!(back.len(), MACRO_SLOTS);
        assert!(back[0].is_empty());
        assert_eq!(back[1], [m2[0], m2[1], m2[2], ev(224, false, 0)]);
        assert_eq!(back[2], m3);
        // Nothing bound: the events at 64 go to the lowest id there.
        let back = parse_macros(&block, &[]);
        assert_eq!(back[0].len(), 4);
        assert!(back[1].is_empty());
        assert_eq!(back[2], m3);
        // Factory-empty block and the mouse type of the official reader.
        assert!(parse_macros(&vec![0; MACRO_LEN], &[]).iter().all(Vec::is_empty));
        let mouse = vec![MacroEvent { key: MacroKey::Mouse(1), press: true, delay_ms: 0 }];
        let raw = encode_macros(std::slice::from_ref(&mouse)).unwrap();
        assert_eq!(raw[64..], [0, 0, 0xC3, 1]);
        assert_eq!(parse_macros(&raw, &[])[0], mouse);
        // Limits and addresses.
        assert_eq!(MACRO_EVENTS_MAX, 496);
        assert!(encode_macros(&[vec![ev(4, true, 1); 496]]).is_some());
        assert!(encode_macros(&[vec![ev(4, true, 1); 497]]).is_none());
        assert_eq!(macro_base(3), 6144);
        // 68 B at 2048 * profile: a full 56 B chunk, then 12 B flagged last.
        let pk = write_region(cmd::MACRO_WRITE, macro_base(1), &raw);
        assert_eq!(pk.len(), 2);
        assert_eq!((pk[0][..3].to_vec(), pk[0][4..8].to_vec()), (vec![GROUP, 0x0D, 0], vec![56, 0x00, 0x08, 0]));
        assert_eq!(pk[1][4..8], [12, 0x38, 0x08, 1]);
        assert_eq!(macro_action(5), Action { kind: 112, code1: 5, code2: 1 });
        assert!(macros_bound(&[vec![macro_action(5)]])[5]);
    }

    #[test]
    fn func_fields() {
        let mut b = [0u8; 64];
        func::set(&mut b, func::TICK, 2);
        func::set(&mut b, func::POLLING, 1);
        assert_eq!(b[4], 0x21);
        func::set(&mut b, func::DEBOUNCE, 2);
        func::set(&mut b, func::ADAPTIVE, 1);
        assert_eq!(b[7], 0x41);
        assert_eq!(func::get(&b, func::DEBOUNCE), 2);
        assert_eq!(func::get(&b, func::POLLING), 1);
    }
}
