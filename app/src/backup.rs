//! Back-up files: every page's settings in one JSON file, for moving to another
//! PC or restoring after a fresh install.
//!
//! Imports are checked value by value. Numbers are clamped to the ranges the
//! UI offers; key actions must be ones Kraken's own pickers offer (or the
//! factory action), so a file can't smuggle in e.g. a reset key. What was
//! changed is reported as notes.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use dd_proto::catalog::CATEGORIES;
use dd_proto::knob::KnobMap;
use dd_proto::led;
use dd_proto::packets::Travel;
use dd_proto::rtp::{self, RdtPair};
use dd_proto::{KeyAction, KeyDef, Keymap, Layer, Model, SLOTS};
use serde::{Deserialize, Serialize};

use crate::store::{AgentConfig, LightPreset1, LightPreset2, Lighting1, Profile, Renderer};
use crate::theme::ThemeId;

pub const FORMAT: &str = "kraken-control-profile";
pub const VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct File {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub app_version: String,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub keyboard: Option<Keyboard>,
    #[serde(default)]
    pub kraken: Option<Kraken>,
}

/// One keyboard profile: everything on the keyboard pages.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "family", rename_all = "snake_case")]
pub enum Keyboard {
    Gen1(Box<Gen1>),
    Gen2(Box<Gen2>),
}

impl Keyboard {
    pub fn name(&self) -> String {
        match self {
            Keyboard::Gen1(g) => g.model.name().to_string(),
            Keyboard::Gen2(g) => g.board.clone(),
        }
    }
}

/// A75 Master / Ultra (gen 1): travel, switches, key pairs, keymap, lighting.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gen1 {
    pub model: Model,
    pub settings: Profile,
    pub lighting: Lighting1,
    /// Knob actions (files from before v0.13 have none).
    #[serde(default)]
    pub knob: Option<KnobMap>,
}

/// 2025 boards (gen 2). Raw keyboard values, but per field, so a file can be
/// checked and applied on top of what the keyboard has.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Gen2 {
    pub board: String,
    /// Per matrix slot.
    pub keys: Vec<Gen2Key>,
    /// Layers × slots, `[kind, code1, code2]`.
    pub keymap: Vec<Vec<[u8; 3]>>,
    /// Keyboard settings and lighting by name (see `ui2::BACKUP_FIELDS`).
    pub settings: BTreeMap<String, u8>,
    pub colors: Vec<[u8; 3]>,
    /// Advanced-key tables (Mod Tap / SOCD / RS / OKS, Toggle).
    pub mt: Vec<u8>,
    pub tgl: Vec<u8>,
    /// DKS table; missing in files written before v0.13.
    #[serde(default)]
    pub dks: Vec<u8>,
    /// Macro block (`gen2::encode_macros`); missing in older files.
    #[serde(default)]
    pub macros: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gen2Key {
    pub actuation: f32,
    /// 0 off, 1 Rapid Trigger, 2 continuous.
    pub mode: u8,
    pub press: f32,
    pub release: f32,
    pub top_dz: f32,
    pub bottom_dz: f32,
    /// SOCD priority.
    pub priority: u8,
}

/// Kraken's own settings.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Kraken {
    #[serde(default)]
    pub theme: ThemeId,
    #[serde(default)]
    pub renderer: Renderer,
    #[serde(default)]
    pub light_presets: Vec<LightPreset1>,
    #[serde(default)]
    pub light_presets2: Vec<LightPreset2>,
    /// Profiles saved on this PC (gen 1).
    #[serde(default)]
    pub profiles: Vec<Profile>,
    #[serde(default)]
    pub agent: AgentConfig,
}

impl Kraken {
    /// "3 lighting presets, 1 saved profile, 2 app rules".
    pub fn summary(&self) -> String {
        let n = |count: usize, one: &str, many: &str| format!("{count} {}", if count == 1 { one } else { many });
        [
            format!("{} theme", self.theme.name()),
            n(self.light_presets.len() + self.light_presets2.len(), "lighting preset", "lighting presets"),
            n(self.profiles.len(), "saved profile", "saved profiles"),
            n(self.agent.rules.len(), "app rule", "app rules"),
        ]
        .join(", ")
    }
}

pub fn new_file(keyboard: Option<Keyboard>, kraken: Option<Kraken>) -> File {
    File {
        format: FORMAT.into(),
        version: VERSION,
        app_version: env!("CARGO_PKG_VERSION").into(),
        created: crate::log::timestamp(),
        keyboard,
        kraken,
    }
}

/// Reads and checks the outer format (contents are checked when applied).
pub fn parse(text: &str) -> Result<File, String> {
    #[derive(Deserialize)]
    struct Head {
        #[serde(default)]
        format: String,
        #[serde(default)]
        version: u32,
    }
    let head: Head = serde_json::from_str(text).map_err(|_| "This isn't a Kraken Control profile file.".to_string())?;
    if head.format != FORMAT {
        return Err("This isn't a Kraken Control profile file.".into());
    }
    if head.version > VERSION {
        return Err(format!("This file is from a newer Kraken Control (format {}). Update Kraken Control to import it.", head.version));
    }
    serde_json::from_str(text).map_err(|e| format!("The file is damaged or incomplete ({e})."))
}

pub fn read(path: &Path) -> Result<File, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("Couldn't open the file: {e}"))?;
    // Real files are ~100 KB; refuse anything absurd before reading it.
    if meta.len() > 8 * 1024 * 1024 {
        return Err("The file is too large to be a Kraken Control profile.".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("Couldn't read the file: {e}"))?;
    parse(&text)
}

/// Writes next to the target and renames, so a failed write never leaves half a file.
pub fn write(path: &Path, file: &File) -> io::Result<()> {
    let json = serde_json::to_string_pretty(file).map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json)?;
    std::fs::rename(&tmp, path)
}

/// "Kraken Control - A75 Master - 2026-11-10.json".
pub fn suggested_name(keyboard: Option<&str>) -> String {
    let date = crate::log::timestamp().chars().take(10).collect::<String>();
    match keyboard {
        Some(k) => format!("Kraken Control - {k} - {date}.json"),
        None => format!("Kraken Control - {date}.json"),
    }
}

// ------------------------------------------------------------------ checks

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn clamp_mm(v: f32, lo: f32, hi: f32, fallback: f32) -> f32 {
    if v.is_finite() {
        v.clamp(lo, hi)
    } else {
        fallback
    }
}

/// Whether a gen-1 key action is one Kraken offers itself.
/// Knob actions must come from the key picker; others are dropped (knob's own action).
pub fn check_knob(k: &mut KnobMap) -> usize {
    let mut dropped = 0;
    for a in k.iter_mut() {
        if a.is_some_and(|x| !x.is_none() && !CATEGORIES.iter().any(|c| c.items.iter().any(|i| i.action == x))) {
            *a = None;
            dropped += 1;
        }
    }
    dropped
}

fn gen1_action_ok(a: KeyAction, factory: KeyAction) -> bool {
    a.is_none() || a == factory || CATEGORIES.iter().any(|c| c.items.iter().any(|i| i.action == a))
}

/// Makes a gen-1 profile safe to send: ranges, pair rules, known key actions.
/// Errs if it can't be used at all (wrong size).
pub fn check_gen1_profile(p: &mut Profile, factory: &Keymap, layout: &[KeyDef]) -> Result<Vec<String>, String> {
    if !p.is_valid() {
        return Err("The key settings in the file are incomplete.".into());
    }
    let mut notes = Vec::new();
    let mut clamped = 0;
    for (kind, values, fallback) in [(Travel::Ap, &mut p.ap, 2.0), (Travel::Ds, &mut p.ds, 0.0), (Travel::Us, &mut p.us, 0.0)] {
        let (lo, hi) = kind.range_mm();
        for v in values.iter_mut() {
            let c = clamp_mm(*v, lo, hi, fallback);
            if c != *v {
                clamped += 1;
                *v = c;
            }
        }
    }
    if clamped > 0 {
        notes.push(format!("{} out of range, adjusted", plural(clamped, "travel value", "travel values")));
    }

    let exists = |s: u8| layout.iter().any(|k| k.slot == s);
    let before = (p.lw.len(), p.rdt.len());
    let mut used = Vec::new();
    p.lw.retain(|&(a, b)| {
        let ok = a != b && exists(a) && exists(b) && !used.contains(&a) && !used.contains(&b);
        if ok {
            used.extend([a, b]);
        }
        ok
    });
    p.lw.truncate(rtp::MAX_LW_PAIRS);
    used.clear();
    p.rdt.retain(|r| {
        let ok = r.main != r.trigger && exists(r.main) && exists(r.trigger) && !used.contains(&r.main) && !used.contains(&r.trigger);
        if ok {
            used.extend([r.main, r.trigger]);
        }
        ok
    });
    p.rdt.truncate(rtp::MAX_RDT_PAIRS);
    for r in &mut p.rdt {
        r.active_mm = clamp_mm(r.active_mm, RdtPair::ACTIVE_RANGE.0, RdtPair::ACTIVE_RANGE.1, RdtPair::ACTIVE_RANGE.0);
        r.reset_mm = clamp_mm(r.reset_mm, RdtPair::RESET_RANGE.0, RdtPair::RESET_RANGE.1, RdtPair::RESET_RANGE.0);
    }
    let dropped = before.0 + before.1 - p.lw.len() - p.rdt.len();
    if dropped > 0 {
        notes.push(format!("{} not valid on this keyboard, left out", plural(dropped, "key pair", "key pairs")));
    }

    // Same rules as the switches on the Performance page.
    let f = &mut p.flags;
    if f.last_win && f.rdt {
        f.rdt = false;
    }
    if (f.last_win || f.rdt) && (!f.rapid_trigger || f.turbo) {
        f.last_win = false;
        f.rdt = false;
        notes.push("key pairs need Rapid Trigger on and Turbo off, so they were turned off".into());
    }
    if !f.rt_match {
        p.match_mode = None;
    }

    let mut reset = 0;
    for layer in Layer::ALL {
        for slot in 0..SLOTS {
            let fac = factory.get(layer, slot);
            if !gen1_action_ok(p.keymap.get(layer, slot), fac) {
                p.keymap.set(layer, slot, fac);
                reset += 1;
            }
        }
    }
    if reset > 0 {
        notes.push(format!("{} not offered by Kraken, set back to factory", plural(reset, "key action", "key actions")));
    }
    Ok(notes)
}

/// Gen-1 lighting within the ranges the Lighting page offers.
pub fn check_lighting1(l: &mut Lighting1) -> bool {
    let mut changed = false;
    if !l.custom && led::effect_by_code(l.effect).is_none() {
        *l = Lighting1 { keys: std::mem::take(&mut l.keys), ..Lighting1::default() };
        changed = true;
    }
    for v in [&mut l.bright, &mut l.speed] {
        if *v > led::MAX_LEVEL {
            *v = led::MAX_LEVEL;
            changed = true;
        }
    }
    if l.colour as usize > led::PRESETS.len() {
        l.colour = 1;
        changed = true;
    }
    if l.keys.len() != SLOTS {
        l.keys.resize(SLOTS, [255, 255, 255]);
        changed = true;
    }
    changed
}

/// Kraken settings: drops what can't be used, fixes the rest. Returns notes.
pub fn check_kraken(k: &mut Kraken) -> Vec<String> {
    let mut notes = Vec::new();
    let before = k.profiles.len();
    k.profiles.retain_mut(|p| {
        let factory = Keymap::factory(p.model);
        check_gen1_profile(p, &factory, &p.model.layout()).is_ok()
    });
    if k.profiles.len() < before {
        notes.push(format!("{} incomplete, left out", plural(before - k.profiles.len(), "saved profile", "saved profiles")));
    }
    for p in &mut k.light_presets {
        check_lighting1(&mut p.light);
    }
    let before = k.light_presets2.len();
    k.light_presets2.retain(|p| p.colors.len() <= 512 && check_gen2_light_fields(&p.fields));
    if k.light_presets2.len() < before {
        notes.push(format!("{} not valid, left out", plural(before - k.light_presets2.len(), "lighting preset", "lighting presets")));
    }
    for r in &mut k.agent.rules {
        r.exe = crate::store::normalize_exe(&r.exe);
        r.slot = r.slot.min(rtp::MAX_SLOTS - 1);
    }
    k.agent.rules.retain(|r| !r.exe.is_empty());
    k.agent.default_slot = k.agent.default_slot.map(|s| s.min(rtp::MAX_SLOTS - 1));
    notes
}

/// Gen-2 lighting preset fields, in `ui2::LIGHTING` order:
/// effect, brightness, speed (inverted), multi, R, G, B.
pub fn check_gen2_light_fields(f: &[u8]) -> bool {
    f.len() == 7 && (f[0] as usize) < dd_proto::gen2::EFFECTS.len() && f[1] <= 100 && f[2] <= 4 && f[3] <= 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_proto::packets::CommonFlags;

    fn profile(model: Model) -> Profile {
        Profile {
            name: String::new(),
            model,
            ap: vec![2.0; SLOTS],
            ds: vec![0.0; SLOTS],
            us: vec![0.0; SLOTS],
            flags: CommonFlags { rapid_trigger: true, ..Default::default() },
            keymap: Keymap::factory(model),
            lw: vec![],
            rdt: vec![],
            match_mode: None,
            lw_replace: false,
        }
    }

    #[test]
    fn round_trip_keeps_everything() {
        let g = Gen1 { model: Model::A75Master, settings: profile(Model::A75Master), lighting: Lighting1::default(), knob: None };
        let k = Kraken { theme: ThemeId::BlackPlumb, ..Default::default() };
        let f = new_file(Some(Keyboard::Gen1(Box::new(g))), Some(k));
        let text = serde_json::to_string(&f).unwrap();
        let back = parse(&text).unwrap();
        assert!(matches!(back.keyboard, Some(Keyboard::Gen1(ref g)) if g.settings.ap.len() == SLOTS));
        assert_eq!(back.kraken.unwrap().theme, ThemeId::BlackPlumb);
    }

    #[test]
    fn rejects_other_files_and_newer_formats() {
        assert!(parse("{}").is_err());
        assert!(parse("not json").is_err());
        assert!(parse(r#"{"format":"something-else","version":1}"#).is_err());
        let newer = parse(&format!(r#"{{"format":"{FORMAT}","version":{}}}"#, VERSION + 1)).unwrap_err();
        assert!(newer.contains("newer"), "{newer}");
        // Only the parts present are imported.
        let empty = parse(&format!(r#"{{"format":"{FORMAT}","version":1}}"#)).unwrap();
        assert!(empty.keyboard.is_none() && empty.kraken.is_none());
    }

    #[test]
    fn gen1_values_are_made_safe() {
        let model = Model::A75Master;
        let factory = Keymap::factory(model);
        let layout = model.layout();
        let mut p = profile(model);
        p.ap[10] = 99.0;
        p.ds[11] = f32::NAN;
        // An action no picker offers (raw code), and a valid catalog one.
        let slot = layout[5].slot as usize;
        p.keymap.set(Layer::Default, slot, KeyAction { cmd: 0xEE, kind: 0xEE, code: 0xEE });
        let ok = CATEGORIES[0].items[0].action;
        p.keymap.set(Layer::Default, layout[6].slot as usize, ok);
        // Bad pairs: same key twice, unknown slot, too many.
        let (a, b) = (layout[0].slot, layout[1].slot);
        p.lw = vec![(a, a), (a, 250), (a, b)];
        p.flags.last_win = true;
        p.flags.rdt = true;
        p.flags.turbo = false;
        let notes = check_gen1_profile(&mut p, &factory, &layout).unwrap();
        assert_eq!(p.ap[10], 3.3);
        assert_eq!(p.ds[11], 0.0);
        assert_eq!(p.keymap.get(Layer::Default, slot), factory.get(Layer::Default, slot));
        assert_eq!(p.keymap.get(Layer::Default, layout[6].slot as usize), ok);
        assert_eq!(p.lw, vec![(a, b)]);
        assert!(p.flags.last_win && !p.flags.rdt);
        assert_eq!(notes.len(), 3, "{notes:?}");
    }

    #[test]
    fn key_pairs_off_without_rapid_trigger() {
        let model = Model::A75Master;
        let mut p = profile(model);
        p.flags = CommonFlags { rapid_trigger: true, turbo: true, last_win: true, ..Default::default() };
        check_gen1_profile(&mut p, &Keymap::factory(model), &model.layout()).unwrap();
        assert!(!p.flags.last_win);
    }

    #[test]
    fn short_profiles_are_refused() {
        let model = Model::A75Master;
        let mut p = profile(model);
        p.ap.truncate(3);
        assert!(check_gen1_profile(&mut p, &Keymap::factory(model), &model.layout()).is_err());
    }

    #[test]
    fn knob_actions_must_be_offered() {
        let offered = CATEGORIES[0].items[0].action;
        let mut k: KnobMap = [None; 6];
        k[0] = Some(offered);
        k[1] = Some(KeyAction::new(0xFD, 2, 5)); // a device function the picker never offers
        k[2] = Some(KeyAction::NONE);
        assert_eq!(check_knob(&mut k), 1);
        assert_eq!(k[0], Some(offered));
        assert_eq!(k[1], None);
        assert_eq!(k[2], Some(KeyAction::NONE));
    }
}
