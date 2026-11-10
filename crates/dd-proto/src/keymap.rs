//! Per-slot key actions and the 3-layer keymap.
//!
//! Wire encoding mirrors `sendRemapKeyData` in the official driver: one
//! 6-byte entry per slot, fixed stride, layout chosen by `kind` (the JS
//! `keyType`). An action with `cmd == 0` is "no entry" and encodes to six
//! zero bytes (the slot byte is NOT written in that case).

use serde::{Deserialize, Serialize};

use crate::model::Model;

/// Number of firmware slots per layer.
pub const SLOTS: usize = 126;

/// keyCmd values seen in the official driver.
pub mod cmd {
    /// Plain HID keyboard usage (and type-1 modifier masks).
    pub const KEY: u8 = 0xFC;
    /// Device function, lighting group (Win lock, LED controls, LW/RDT toggles).
    pub const DEVICE: u8 = 0xFB;
    /// Device function, performance group (RT, RT match modes, reset, turbo).
    pub const PERF: u8 = 0xFD;
    /// Layer key Fn.
    pub const FN: u8 = 0xFF;
    /// Layer key Fn2.
    pub const FN2: u8 = 0xFE;
    /// LW / RDT pair entry (keyType 4).
    pub const PAIR: u8 = 0xF8;
}

/// One remap entry. `kind` is the official `keyType`:
/// 0 = HID usage in byte 3, 1/2 = value in byte 2 (modifier mask / device
/// function code), 3 = command only (layer keys, media, mouse).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct KeyAction {
    pub cmd: u8,
    pub kind: u8,
    pub code: u8,
}

impl KeyAction {
    pub const NONE: Self = Self { cmd: 0, kind: 0, code: 0 };

    pub const fn new(cmd: u8, kind: u8, code: u8) -> Self {
        Self { cmd, kind, code }
    }

    /// Plain HID keyboard-page usage (keyType 0).
    pub const fn hid(usage: u8) -> Self {
        Self { cmd: cmd::KEY, kind: 0, code: usage }
    }

    /// Modifier as the official default layer encodes it (keyType 1, mask in byte 2).
    pub const fn modifier(mask: u8) -> Self {
        Self { cmd: cmd::KEY, kind: 1, code: mask }
    }

    pub fn is_none(&self) -> bool {
        self.cmd == 0
    }

    /// 6-byte wire entry for `slot`.
    pub fn encode(&self, slot: u8) -> [u8; 6] {
        if self.cmd == 0 {
            return [0; 6];
        }
        match self.kind {
            0 => [slot, self.cmd, 0, self.code, 0, 0],
            1 | 2 => [slot, self.cmd, self.code, 0, 0, 0],
            _ => [slot, self.cmd, 0, 0, 0, 0],
        }
    }
}

/// Remap layer. Wire `group` byte = index + 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Layer {
    Default = 0,
    Fn = 1,
    Fn2 = 2,
}

impl Layer {
    pub const ALL: [Layer; 3] = [Layer::Default, Layer::Fn, Layer::Fn2];

    pub fn group(self) -> u8 {
        self as u8 + 1
    }

    pub fn name(self) -> &'static str {
        match self {
            Layer::Default => "Default",
            Layer::Fn => "Fn",
            Layer::Fn2 => "Fn2",
        }
    }
}

/// Full keymap: 3 layers x 126 slots.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keymap {
    #[serde(with = "layers_serde")]
    pub layers: [[KeyAction; SLOTS]; 3],
}

impl Keymap {
    pub fn empty() -> Self {
        Self { layers: [[KeyAction::NONE; SLOTS]; 3] }
    }

    /// Factory keymap from the official driver's tables.
    pub fn factory(model: Model) -> Self {
        let raw = match model {
            Model::A75Master => include_str!("../data/a75master-keymap.json"),
            Model::A75Ultra => include_str!("../data/a75ultra-keymap.json"),
        };
        parse_official(raw).expect("embedded keymap data is valid")
    }

    pub fn get(&self, layer: Layer, slot: usize) -> KeyAction {
        self.layers[layer as usize][slot]
    }

    pub fn set(&mut self, layer: Layer, slot: usize, action: KeyAction) {
        self.layers[layer as usize][slot] = action;
    }
}

#[derive(Deserialize)]
struct OfficialFile {
    layers: OfficialLayers,
}

#[derive(Deserialize)]
struct OfficialLayers {
    default: Vec<OfficialEntry>,
    fn1: Vec<OfficialEntry>,
    fn2: Vec<OfficialEntry>,
}

#[derive(Deserialize)]
struct OfficialEntry {
    slot: usize,
    cmd: u8,
    #[serde(rename = "type")]
    kind: u8,
    code: i64,
}

/// Parses the extracted official keymap JSON (`docs/data/a75*-keymap.json`).
pub fn parse_official(json: &str) -> Result<Keymap, String> {
    let f: OfficialFile = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut km = Keymap::empty();
    for (i, entries) in [f.layers.default, f.layers.fn1, f.layers.fn2].into_iter().enumerate() {
        for e in entries {
            if e.slot >= SLOTS {
                return Err(format!("slot {} out of range", e.slot));
            }
            let code = u8::try_from(e.code).map_err(|_| format!("code {} out of range", e.code))?;
            km.layers[i][e.slot] = KeyAction::new(e.cmd, e.kind, code);
        }
    }
    Ok(km)
}

mod layers_serde {
    //! serde has no impls for arrays longer than 32; go through Vec.
    use super::{KeyAction, SLOTS};
    use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(v: &[[KeyAction; SLOTS]; 3], s: S) -> Result<S::Ok, S::Error> {
        let as_vec: Vec<Vec<KeyAction>> = v.iter().map(|l| l.to_vec()).collect();
        as_vec.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<[[KeyAction; SLOTS]; 3], D::Error> {
        let v: Vec<Vec<KeyAction>> = Vec::deserialize(d)?;
        if v.len() != 3 {
            return Err(D::Error::custom("expected 3 layers"));
        }
        let mut out = [[KeyAction::NONE; SLOTS]; 3];
        for (i, layer) in v.into_iter().enumerate() {
            if layer.len() != SLOTS {
                return Err(D::Error::custom("expected 126 slots per layer"));
            }
            out[i].copy_from_slice(&layer);
        }
        Ok(out)
    }
}
