//! Persistent app state: working keymaps per model and named profiles.
//! Stored as JSON in %APPDATA%\KrakenControl\state.json.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use dd_proto::knob::KnobMap;
use dd_proto::packets::CommonFlags;
use dd_proto::rtp::{MatchMode, RdtPair};
use dd_proto::{Keymap, Model, SLOTS};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub model: Model,
    pub ap: Vec<f32>,
    pub ds: Vec<f32>,
    pub us: Vec<f32>,
    pub flags: CommonFlags,
    pub keymap: Keymap,
    #[serde(default)]
    pub lw: Vec<(u8, u8)>,
    #[serde(default)]
    pub rdt: Vec<RdtPair>,
    #[serde(default)]
    pub match_mode: Option<MatchMode>,
    #[serde(default)]
    pub lw_replace: bool,
}

impl Profile {
    pub fn is_valid(&self) -> bool {
        self.ap.len() == SLOTS && self.ds.len() == SLOTS && self.us.len() == SLOTS
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub log_packets: bool,
    /// UI theme (Dark Grey unless changed in Settings).
    #[serde(default)]
    pub theme: crate::theme::ThemeId,
    /// How the window is drawn (takes effect at the next start).
    #[serde(default)]
    pub renderer: Renderer,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Renderer {
    /// CPU drawing, no graphics driver loaded (much less memory).
    #[default]
    Software,
    /// OpenGL through eframe.
    Gpu,
}

/// Gen-1 lighting as Kraken last sent it (the keyboard can't report it).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lighting1 {
    /// False until Kraken has sent lighting to this model.
    pub sent: bool,
    /// Per-key "Custom light"; else the firmware effect `effect`.
    pub custom: bool,
    pub effect: u8,
    pub bright: u8,
    pub speed: u8,
    /// 0 = colourful, 1..=7 = preset (`dd_proto::led::PRESETS`).
    pub colour: u8,
    /// Per-key colours by slot.
    pub keys: Vec<[u8; 3]>,
}

impl Default for Lighting1 {
    fn default() -> Self {
        Self { sent: false, custom: false, effect: 0, bright: 9, speed: 5, colour: 1, keys: vec![[255, 255, 255]; SLOTS] }
    }
}

/// Named gen-1 lighting look (Lighting page presets).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LightPreset1 {
    pub name: String,
    pub model: Model,
    pub light: Lighting1,
}

/// Named gen-2 lighting look: the Lighting fields (order of `ui2::LIGHTING`)
/// and the per-key colours.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LightPreset2 {
    pub name: String,
    /// Keyboard name, e.g. "A75 Ultra (2025)".
    pub board: String,
    pub fields: Vec<u8>,
    pub colors: Vec<[u8; 3]>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub settings: Settings,
    /// Last keymap written to (or edited for) each model. The keymap cannot
    /// be read back from the keyboard, so this is our record of it.
    #[serde(default)]
    pub keymaps: Vec<(Model, Keymap)>,
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Gen-1 lighting per model, as last sent.
    #[serde(default)]
    pub lighting: Vec<(Model, Lighting1)>,
    /// Lighting presets, gen-1 and gen-2.
    #[serde(default)]
    pub light_presets: Vec<LightPreset1>,
    #[serde(default)]
    pub light_presets2: Vec<LightPreset2>,
    /// Gen-1 knob per model, as last written (not readable from the keyboard).
    #[serde(default)]
    pub knobs: Vec<(Model, KnobMap)>,
}

impl Store {
    pub fn path() -> PathBuf {
        data_dir().join("state.json")
    }

    pub fn load() -> Store {
        fs::read_to_string(Self::path())
            .ok()
            .and_then(|s| serde_json::from_str::<Store>(&s).ok())
            .map(|mut s| {
                s.profiles.retain(Profile::is_valid);
                s
            })
            .unwrap_or_default()
    }

    pub fn save(&self) -> io::Result<()> {
        write_atomic(&Self::path(), &serde_json::to_vec_pretty(self)?)
    }

    pub fn keymap(&self, model: Model) -> Keymap {
        self.keymaps
            .iter()
            .find(|(m, _)| *m == model)
            .map(|(_, k)| k.clone())
            .unwrap_or_else(|| Keymap::factory(model))
    }

    pub fn lighting(&self, model: Model) -> Lighting1 {
        let mut l = self.lighting.iter().find(|(m, _)| *m == model).map(|(_, l)| l.clone()).unwrap_or_default();
        l.keys.resize(SLOTS, [255, 255, 255]);
        l
    }

    pub fn set_lighting(&mut self, model: Model, l: &Lighting1) {
        match self.lighting.iter_mut().find(|(m, _)| *m == model) {
            Some((_, x)) => *x = l.clone(),
            None => self.lighting.push((model, l.clone())),
        }
    }

    pub fn knob(&self, model: Model) -> KnobMap {
        self.knobs.iter().find(|(m, _)| *m == model).map(|(_, k)| *k).unwrap_or_default()
    }

    pub fn set_knob(&mut self, model: Model, k: &KnobMap) {
        match self.knobs.iter_mut().find(|(m, _)| *m == model) {
            Some((_, x)) => *x = *k,
            None => self.knobs.push((model, *k)),
        }
    }

    pub fn set_keymap(&mut self, model: Model, km: &Keymap) {
        match self.keymaps.iter_mut().find(|(m, _)| *m == model) {
            Some((_, k)) => *k = km.clone(),
            None => self.keymaps.push((model, km.clone())),
        }
    }
}

const APP_DIR: &str = "KrakenControl";
/// Settings folder before the rename to Kraken Control (v0.3 and earlier).
const LEGACY_DIR: &str = "DeerControl";

fn appdata() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Folder for state.json, agent.json and log.txt: `<exe folder>\data` in a
/// portable copy, else `%APPDATA%\KrakenControl`.
pub fn data_dir() -> PathBuf {
    portable_dir().unwrap_or_else(|| appdata().join(APP_DIR))
}

/// A file named `portable` next to the exe makes the copy portable: settings
/// and log stay in `data\` beside it (nothing written to %APPDATA%).
pub fn portable_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    dir.join("portable").is_file().then(|| dir.join("data"))
}

/// Moves `%APPDATA%\DeerControl` to `%APPDATA%\KrakenControl` once. Returns a
/// note for the log. Falls back to copying the settings files if the folder
/// can't be moved (e.g. a file still open).
pub fn migrate_legacy_dir() -> Option<String> {
    if portable_dir().is_some() {
        return None;
    }
    let (old, new) = (appdata().join(LEGACY_DIR), appdata().join(APP_DIR));
    if new.exists() || !old.exists() {
        return None;
    }
    match fs::rename(&old, &new) {
        Ok(()) => Some(format!("moved settings from {}", old.display())),
        Err(e) => {
            let _ = fs::create_dir_all(&new);
            for f in ["state.json", "agent.json"] {
                let _ = fs::copy(old.join(f), new.join(f));
            }
            Some(format!("copied settings from {} ({e})", old.display()))
        }
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(tmp, path)
}

/// Per-app slot rule: when `exe` (lower-case file name) is in the foreground,
/// switch to onboard slot `slot` (0-based).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRule {
    pub exe: String,
    pub slot: u8,
}

/// Tray-agent settings, in their own file (`agent.json`) so the UI and the
/// agent each read-modify-write only this small file.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default)]
    pub auto_switch: bool,
    /// Slot for apps without a rule; `None` keeps the current slot.
    #[serde(default)]
    pub default_slot: Option<u8>,
    #[serde(default)]
    pub rules: Vec<AppRule>,
}

impl AgentConfig {
    pub fn path() -> PathBuf {
        Store::path().with_file_name("agent.json")
    }

    pub fn load() -> AgentConfig {
        fs::read_to_string(Self::path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> io::Result<()> {
        write_atomic(&Self::path(), &serde_json::to_vec_pretty(self)?)
    }

    /// Modification time, to notice edits made by the other process.
    pub fn stamp() -> Option<SystemTime> {
        fs::metadata(Self::path()).and_then(|m| m.modified()).ok()
    }

    pub fn target_for(&self, exe: &str) -> Option<u8> {
        self.rules.iter().find(|r| r.exe.eq_ignore_ascii_case(exe)).map(|r| r.slot).or(self.default_slot)
    }
}

/// Normalises user input to a lower-case exe file name.
pub fn normalize_exe(s: &str) -> String {
    let name = s.trim().rsplit(['\\', '/']).next().unwrap_or("").to_lowercase();
    if name.is_empty() || name.ends_with(".exe") {
        name
    } else {
        format!("{name}.exe")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_and_default() {
        let c = AgentConfig {
            auto_switch: true,
            default_slot: Some(0),
            rules: vec![AppRule { exe: "game.exe".into(), slot: 2 }],
        };
        assert_eq!(c.target_for("GAME.EXE"), Some(2));
        assert_eq!(c.target_for("other.exe"), Some(0));
        assert_eq!(AgentConfig::default().target_for("x.exe"), None);
    }

    #[test]
    fn exe_names() {
        assert_eq!(normalize_exe(" C:\\Games\\Valorant.EXE "), "valorant.exe");
        assert_eq!(normalize_exe("cs2"), "cs2.exe");
        assert_eq!(normalize_exe("  "), "");
    }
}
