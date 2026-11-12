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
    /// From a profile exported by the official gen-2 web driver: it names no
    /// keyboard, so it is offered to whichever gen-2 board is connected.
    pub fn official(&self) -> bool {
        matches!(self, Keyboard::Gen2(g) if g.board == OFFICIAL_BOARD)
    }

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
    if head.format.is_empty() {
        if let Some(official) = parse_official(text) {
            let (g, created) = official?;
            return Ok(File {
                format: FORMAT.into(),
                version: VERSION,
                app_version: String::new(),
                created,
                keyboard: Some(Keyboard::Gen2(Box::new(g))),
                kraken: None,
            });
        }
    }
    if head.format != FORMAT {
        return Err("This isn't a Kraken Control profile file.".into());
    }
    if head.version > VERSION {
        return Err(format!("This file is from a newer Kraken Control (format {}). Update Kraken Control to import it.", head.version));
    }
    serde_json::from_str(text).map_err(|e| format!("The file is damaged or incomplete ({e})."))
}

/// `Gen2::board` of a file exported by the official 2025 web driver (Profile > Export).
/// That file doesn't say which keyboard it is from; the layout checks still apply.
pub const OFFICIAL_BOARD: &str = "official driver export";

/// A profile exported by the official gen-2 web driver: `{ name, travelKeys,
/// advancedKeys, userKeys, exportTime }` (see docs/protocol-gen2.md "Official
/// config export"). `None` if `text` isn't one. Tables and advanced keymap
/// entries are rebuilt from the `advancedKeys` list the way the driver's own
/// import does; everything then goes through the normal import checks.
pub fn parse_official(text: &str) -> Option<Result<(Gen2, String), String>> {
    use dd_proto::gen2::{self, Action};
    use serde_json::Value;
    let v: Value = serde_json::from_str(text).ok()?;
    let user = v.get("userKeys")?.as_object()?;
    let travel = v.get("travelKeys")?.as_array()?;
    let num = |v: &Value, key: &str| v.get(key).and_then(Value::as_f64).map_or(0.0, |f| f.max(0.0));
    let byte = |v: &Value, key: &str| num(v, key).min(255.0) as u8;
    let key = |v: Option<&Value>| v.map_or(Action::default(), |k| Action { kind: byte(k, "type"), code1: byte(k, "code1"), code2: byte(k, "code2") });
    let bad = |what: &str| format!("This official driver file can't be used: {what}.");
    let convert = || -> Result<Gen2, String> {
        if travel.len() < gen2::KEYS {
            return Err(bad("it has no key travel data"));
        }
        let keys = travel[..gen2::KEYS]
            .iter()
            .map(|t| Gen2Key {
                actuation: num(t, "key_actuation") as f32 / 100.0,
                mode: byte(t, "key_mode") & 0x0F,
                press: num(t, "rt_press") as f32 / 100.0,
                release: num(t, "rt_release") as f32 / 100.0,
                top_dz: num(t, "press_deadzone") as f32 / 100.0,
                bottom_dz: num(t, "release_deadzone") as f32 / 100.0,
                priority: byte(t, "priority"),
            })
            .collect();
        let mut keymap = Vec::new();
        for layer in 0..4 {
            let list = user.get(&layer.to_string()).and_then(Value::as_array).filter(|l| l.len() >= gen2::KEYS).ok_or_else(|| bad("a key layer is missing or too short"))?;
            let mut actions: Vec<[u8; 3]> = list[..gen2::KEYS].iter().map(|k| key(Some(k))).map(|a| [a.kind, a.code1, a.code2]).collect();
            // Advanced entries are rebuilt from the list below; one left over here
            // gets an index no table has, so the import puts the key back to factory.
            for a in actions.iter_mut().filter(|a| (gen2::kind::DKS..=gen2::kind::OKS).contains(&a[0])) {
                a[1] = 255;
            }
            keymap.push(actions);
        }
        let (mut mt, mut tgl, mut dks) = (vec![0u8; gen2::MT_LEN], vec![0u8; gen2::TGL_LEN], vec![0u8; gen2::DKS_LEN]);
        let (mut n_mt, mut n_tgl, mut n_dks) = (0usize, 0usize, 0usize);
        let full = || bad("it has more advanced keys than the keyboard holds");
        for item in v.get("advancedKeys").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
            let (layer, a, b) = (num(item, "layer") as usize, num(item, "index1") as usize, num(item, "index2") as usize);
            if layer >= 4 || a >= gen2::KEYS || b >= gen2::KEYS {
                return Err(bad("an advanced key points outside the keyboard"));
            }
            match item.get("type").and_then(Value::as_str).unwrap_or("") {
                "dks" => {
                    if n_dks >= gen2::ADV_ENTRIES {
                        return Err(full());
                    }
                    let mut d = gen2::Dks::default();
                    for (i, p) in item.get("dksPoint").and_then(Value::as_array).into_iter().flatten().take(4).enumerate() {
                        d.points[i] = p.as_f64().map_or(0, |f| f.clamp(0.0, 255.0) as u8);
                    }
                    for (i, k) in item.get("dksKeys").and_then(Value::as_array).into_iter().flatten().take(4).enumerate() {
                        // The driver keeps continuation values under long holds; Kraken's spans don't.
                        let [ds, mut de, mut us, ue] = ["downStart", "downEnd", "upStart", "upEnd"].map(|f| byte(k, f));
                        if ds >= 3 {
                            de = 0;
                        }
                        if ds >= 4 || de >= 3 {
                            us = 0;
                        }
                        d.binds[i] = gen2::DksBind { action: key(k.get("key")), spans: gen2::dks_normalize([ds, de, us, ue]) };
                    }
                    gen2::set_dks_entry(&mut dks, n_dks, &d);
                    keymap[layer][a] = [gen2::kind::DKS, n_dks as u8, 0];
                    n_dks += 1;
                }
                "tgl" => {
                    if n_tgl >= gen2::ADV_ENTRIES {
                        return Err(full());
                    }
                    gen2::set_tgl_entry(&mut tgl, n_tgl, key(item.get("tglKey")));
                    keymap[layer][a] = [gen2::kind::TGL, n_tgl as u8, 0];
                    n_tgl += 1;
                }
                "mt" => {
                    if n_mt >= gen2::ADV_ENTRIES {
                        return Err(full());
                    }
                    gen2::set_mt_entry(&mut mt, n_mt, key(item.get("mtClickKey")), key(item.get("mtDownKey")));
                    let ms = match num(item, "mtTime") {
                        0.0 => 100.0,
                        ms => ms,
                    };
                    keymap[layer][a] = [gen2::kind::MT, n_mt as u8, (ms / 10.0).min(255.0) as u8];
                    n_mt += 1;
                }
                pair @ ("rs" | "socd" | "oks") => {
                    if n_mt + 2 > gen2::ADV_ENTRIES {
                        return Err(full());
                    }
                    let kind = match pair {
                        "rs" => gen2::kind::RS,
                        "socd" => gen2::kind::SOCD,
                        _ => gen2::kind::OKS,
                    };
                    let (k1, k2) = (key(item.get("key1")), key(item.get("key2")));
                    gen2::set_mt_entry(&mut mt, n_mt, k1, k2);
                    gen2::set_mt_entry(&mut mt, n_mt + 1, k2, k1);
                    keymap[layer][a] = [kind, n_mt as u8, b as u8];
                    keymap[layer][b] = [kind, n_mt as u8 + 1, a as u8];
                    n_mt += 2;
                }
                _ => {}
            }
        }
        Ok(Gen2 { board: OFFICIAL_BOARD.into(), keys, keymap, settings: BTreeMap::new(), colors: Vec::new(), mt, tgl, dks, macros: Vec::new() })
    };
    let created = v.get("exportTime").and_then(Value::as_str).unwrap_or_default().to_string();
    Some(convert().map(|g| (g, created)))
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

    #[test]
    fn official_export_is_converted() {
        use serde_json::json;
        let key = |t: u8, c1: u8, c2: u8| json!({"type": t, "code1": c1, "code2": c2, "name": "x"});
        let layer = |n: u8| -> Vec<serde_json::Value> { (0..128u8).map(|i| key(16, 0, 4 + (i + n) % 60)).collect() };
        let mut l0 = layer(0);
        l0[5] = key(148, 0, 6); // SOCD entry as it was at export time (also in the list)
        l0[9] = key(146, 7, 20); // Mod Tap entry with nothing in the list
        let travel: Vec<_> = (0..128)
            .map(|_| json!({"switch_type": 0, "key_mode": 1, "priority": 0, "key_max_length": 4, "key_actuation": 150, "pressPrecision": 0, "releasePrecision": 0,
                "rt_press": 30, "rt_release": 25, "press_deadzone": 10, "release_deadzone": 5, "deadzone_status": true}))
            .collect();
        let file = json!({
            "name": "Game", "exportTime": "2026-11-11T10:00:00.000Z",
            "travelKeys": travel,
            "userKeys": {"0": l0, "1": layer(1), "2": layer(2), "3": layer(3)},
            "advancedKeys": [
                {"type": "socd", "layer": 0, "index1": 5, "index2": 6, "key1": key(16, 0, 4), "key2": key(16, 0, 7)},
                {"type": "mt", "layer": 0, "index1": 12, "mtClickKey": key(16, 0, 41), "mtDownKey": key(16, 1, 0), "mtTime": 200},
                {"type": "tgl", "layer": 0, "index1": 20, "tglKey": key(16, 0, 44)},
                {"type": "dks", "layer": 0, "index1": 30, "dksPoint": [10, 30, 30, 10], "dksKeys": [
                    {"key": key(16, 0, 4), "downStart": 3, "downEnd": 2, "upStart": 0, "upEnd": 0},
                    {"key": key(16, 0, 5), "downStart": 0, "downEnd": 0, "upStart": 0, "upEnd": 1}]},
            ]
        });
        let f = parse(&file.to_string()).unwrap();
        assert_eq!(f.created, "2026-11-11T10:00:00.000Z");
        assert!(f.kraken.is_none());
        let Some(Keyboard::Gen2(g)) = f.keyboard else { panic!("gen-2 keyboard part expected") };
        assert_eq!(g.board, OFFICIAL_BOARD);
        assert!(g.colors.is_empty() && g.settings.is_empty() && g.macros.is_empty());
        // Travel: 0.01 mm units -> mm.
        assert_eq!(g.keys.len(), 128);
        let k = g.keys[0];
        assert_eq!((k.mode, k.priority), (1, 0));
        for (got, want) in [(k.actuation, 1.5), (k.press, 0.3), (k.release, 0.25), (k.top_dz, 0.1), (k.bottom_dz, 0.05)] {
            assert!((got - want).abs() < 1e-6, "{got} vs {want}");
        }
        // Keymap entries and tables rebuilt from the list, in list order (pairs take two MT entries).
        assert_eq!(g.keymap.len(), 4);
        assert_eq!(g.keymap[0][5], [148, 0, 6]);
        assert_eq!(g.keymap[0][6], [148, 1, 5]);
        assert_eq!(g.keymap[0][12], [146, 2, 20]);
        assert_eq!(g.keymap[0][20], [145, 0, 0]);
        assert_eq!(g.keymap[0][30], [144, 0, 0]);
        assert_eq!(g.keymap[0][9], [146, 255, 20]); // not in the list: index no table has
        assert_eq!(g.keymap[1][0], [16, 0, 5]);
        assert_eq!(g.mt[..18], [16, 0, 4, 16, 0, 7, 16, 0, 7, 16, 0, 4, 16, 0, 41, 16, 1, 0]);
        assert_eq!(g.tgl[..3], [16, 0, 44]);
        // DKS: the driver's 3 + continuation 2 is one hold over three stages.
        assert_eq!(g.dks[..14], [10, 30, 30, 10, 16, 0, 4, 0x1F, 0, 16, 0, 5, 0x00, 0x02]);
        assert_eq!(dd_proto::gen2::dks_entry(&g.dks, 0).binds[0].spans, [3, 0, 0, 0]);

        // Other JSON is neither format; a short layer is refused with a reason.
        assert!(parse_official("{\"format\":\"x\"}").is_none());
        assert!(parse("{}").unwrap_err().contains("Kraken Control profile"));
        let mut short = file.clone();
        short["userKeys"]["2"] = json!([key(16, 0, 4)]);
        assert!(parse(&short.to_string()).unwrap_err().contains("key layer"));
    }
}
