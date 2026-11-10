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
    /// Per-key colours (effect "Custom"), 128 × RGB at 512·profile.
    pub const COLORS_READ: u8 = 0x0A;
    pub const COLORS_WRITE: u8 = 0x0B;
    /// Advanced-key tables (per profile): DKS 768 B, MT/RS/SOCD/OKS 256 B, TGL 128 B.
    pub const DKS_READ: u8 = 0xA2;
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
    data.chunks_exact(TRIGGER_BYTES).take(KEYS).map(|c| Trigger(c.try_into().unwrap())).collect()
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
    data.chunks_exact(3).take(KEYS).map(Action::from_bytes).collect()
}

/// 128 × 3 bytes. Slots past the matrix hold filler (`F0+layer` ×3); they are
/// written back exactly as read.
pub fn encode_keymap(actions: &[Action]) -> Vec<u8> {
    actions.iter().flat_map(|a| [a.kind, a.code1, a.code2]).collect()
}

pub fn parse_colors(data: &[u8]) -> Vec<[u8; 3]> {
    data.chunks_exact(3).take(KEYS).map(|c| [c[0], c[1], c[2]]).collect()
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
    let o = 24 * i;
    [4, 9, 14, 19].map(|a| Action::from_bytes(&raw[o + a..o + a + 3]))
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
        let key = |c: u8| PickItem { label: basic_name(c).unwrap_or_default(), action: Action { kind: kind::KEY, code1: 0, code2: c } };
        let range = |r: &[std::ops::RangeInclusive<u8>]| -> Vec<PickItem> { r.iter().cloned().flatten().filter(|c| basic_name(*c).is_some()).map(key).collect() };
        let mut letters: Vec<PickItem> = range(&[4..=29]);
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
            ("Numbers & symbols", range(&[30..=39, 45..=49, 51..=56])),
            ("Editing & navigation", range(&[40..=44, 57..=57, 70..=82, 101..=101])),
            ("F-keys", range(&[58..=69, 104..=115])),
            ("Modifiers", mods),
            ("Layers & keyboard", functions),
            ("Numpad", range(&[83..=99])),
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
