//! Model identification and physical key layout.

use serde::{Deserialize, Serialize};

/// Supported models. Detection is by identity type bytes, never by USB PID.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Model {
    /// Type bytes `0b 05 04` (official TypeCode 757).
    A75Master,
    /// Type bytes `0b 04 04` (official TypeCode 756).
    A75Ultra,
}

impl Model {
    pub fn from_type_bytes(b: [u8; 3]) -> Option<Model> {
        match b {
            [11, 5, 4] => Some(Model::A75Master),
            [11, 4, 4] => Some(Model::A75Ultra),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Model::A75Master => "A75 Master",
            Model::A75Ultra => "A75 Ultra",
        }
    }

    /// Physical keys with their firmware slot and drawing geometry (key units).
    pub fn layout(self) -> Vec<KeyDef> {
        layout(self)
    }
}

/// Parsed identity reply (`A0 02 00 ...`), field offsets from the official
/// input-report handler (`gC`, case 160).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    pub type_bytes: [u8; 3],
    pub model: Option<Model>,
    /// Official firmware number: decimal concatenation of bytes 8 and 7
    /// (`Number(b8.toString() + b7.toString())`), e.g. `00 55` -> 85.
    pub firmware: u32,
    /// Display string as the official driver shows it, e.g. "0.085".
    pub firmware_label: String,
    pub turbo: bool,
    pub rapid_trigger: bool,
    pub rdt: bool,
    pub last_win: bool,
    pub profile_index: u8,
    pub light_mode: u8,
    pub light_speed: u8,
    pub light_brightness: u8,
    pub rt_match: bool,
    pub auto_match_mode: u8,
    pub lw_replace: bool,
}

impl Identity {
    pub fn parse(r: &[u8]) -> Option<Identity> {
        if r.len() < 37 || r[0] != 0xA0 || r[1] != 0x02 || r[2] != 0x00 {
            return None;
        }
        let type_bytes = [r[4], r[5], r[6]];
        let (b7, b8) = (r[7], r[8]);
        let firmware: u32 = format!("{b8}{b7}").parse().unwrap_or(0);
        let mut firmware_label = format!("0.{b8}{b7}");
        if r[34] != 0 {
            firmware_label.push_str(&format!("-{:x}.{:x}.{:x}", r[34], r[35], r[36]));
        }
        Some(Identity {
            type_bytes,
            model: Model::from_type_bytes(type_bytes),
            firmware,
            firmware_label,
            turbo: r[15] != 0,
            rapid_trigger: r[16] != 0,
            rdt: r[18] != 0,
            last_win: r[19] != 0,
            profile_index: r[20],
            light_mode: r[22],
            light_speed: r[23],
            light_brightness: r[24],
            rt_match: r[30] != 0,
            auto_match_mode: r[31],
            lw_replace: r[32] != 0,
        })
    }

    /// Firmware >= 81 manages profiles on the keyboard (`event_send_profile_len`).
    pub fn has_onboard_profiles(&self) -> bool {
        self.firmware >= 81
    }

    /// 63 < fw < 81: the official driver prompts for a firmware upgrade.
    pub fn upgrade_recommended(&self) -> bool {
        self.firmware > 63 && self.firmware < 81
    }
}

/// One physical key.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyDef {
    pub slot: u8,
    pub label: &'static str,
    /// Position and size in key units (1.0 = one standard key pitch).
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

const fn k(slot: u8, label: &'static str, x: f32, y: f32, w: f32) -> KeyDef {
    KeyDef { slot, label, x, y, w, h: 1.0 }
}

/// Position of the rotary knob (drawn, not a slot), key units.
pub const KNOB_POS: (f32, f32) = (15.25, 0.0);

fn layout(model: Model) -> Vec<KeyDef> {
    // Slot numbers: official `getA75Master()` / `getA75Ultra()` UI layouts.
    // Geometry: standard 75% pitch; 16.25 units wide.
    let (r0, r1, r2, r3, r4, r5) = (0.0, 1.25, 2.25, 3.25, 4.25, 5.25);
    let mut v = vec![
        // Row 0
        k(0, if model == Model::A75Master { "★" } else { "Esc" }, 0.0, r0, 1.0),
        k(2, "F1", 1.25, r0, 1.0),
        k(3, "F2", 2.25, r0, 1.0),
        k(4, "F3", 3.25, r0, 1.0),
        k(5, "F4", 4.25, r0, 1.0),
        k(6, "F5", 5.5, r0, 1.0),
        k(7, "F6", 6.5, r0, 1.0),
        k(8, "F7", 7.5, r0, 1.0),
        k(9, "F8", 8.5, r0, 1.0),
        k(10, "F9", 9.75, r0, 1.0),
        k(11, "F10", 10.75, r0, 1.0),
        k(12, "F11", 11.75, r0, 1.0),
        k(13, "F12", 12.75, r0, 1.0),
        // Row 1
        k(21, "`", 0.0, r1, 1.0),
        k(22, "1", 1.0, r1, 1.0),
        k(23, "2", 2.0, r1, 1.0),
        k(24, "3", 3.0, r1, 1.0),
        k(25, "4", 4.0, r1, 1.0),
        k(26, "5", 5.0, r1, 1.0),
        k(27, "6", 6.0, r1, 1.0),
        k(28, "7", 7.0, r1, 1.0),
        k(29, "8", 8.0, r1, 1.0),
        k(30, "9", 9.0, r1, 1.0),
        k(31, "0", 10.0, r1, 1.0),
        k(32, "-", 11.0, r1, 1.0),
        k(33, "=", 12.0, r1, 1.0),
        k(34, "Backspace", 13.0, r1, 2.0),
        // Row 2
        k(42, "Tab", 0.0, r2, 1.5),
        k(43, "Q", 1.5, r2, 1.0),
        k(44, "W", 2.5, r2, 1.0),
        k(45, "E", 3.5, r2, 1.0),
        k(46, "R", 4.5, r2, 1.0),
        k(47, "T", 5.5, r2, 1.0),
        k(48, "Y", 6.5, r2, 1.0),
        k(49, "U", 7.5, r2, 1.0),
        k(50, "I", 8.5, r2, 1.0),
        k(51, "O", 9.5, r2, 1.0),
        k(52, "P", 10.5, r2, 1.0),
        k(53, "[", 11.5, r2, 1.0),
        k(54, "]", 12.5, r2, 1.0),
        k(55, "\\", 13.5, r2, 1.5),
        k(57, "PgUp", 15.25, r2, 1.0),
        // Row 3
        k(63, "Caps", 0.0, r3, 1.75),
        k(64, "A", 1.75, r3, 1.0),
        k(65, "S", 2.75, r3, 1.0),
        k(66, "D", 3.75, r3, 1.0),
        k(67, "F", 4.75, r3, 1.0),
        k(68, "G", 5.75, r3, 1.0),
        k(69, "H", 6.75, r3, 1.0),
        k(70, "J", 7.75, r3, 1.0),
        k(71, "K", 8.75, r3, 1.0),
        k(72, "L", 9.75, r3, 1.0),
        k(73, ";", 10.75, r3, 1.0),
        k(74, "'", 11.75, r3, 1.0),
        k(76, "Enter", 12.75, r3, 2.25),
        k(78, "PgDn", 15.25, r3, 1.0),
        // Row 4
        k(84, "Shift", 0.0, r4, 2.25),
        k(86, "Z", 2.25, r4, 1.0),
        k(87, "X", 3.25, r4, 1.0),
        k(88, "C", 4.25, r4, 1.0),
        k(89, "V", 5.25, r4, 1.0),
        k(90, "B", 6.25, r4, 1.0),
        k(91, "N", 7.25, r4, 1.0),
        k(92, "M", 8.25, r4, 1.0),
        k(93, ",", 9.25, r4, 1.0),
        k(94, ".", 10.25, r4, 1.0),
        k(95, "/", 11.25, r4, 1.0),
        k(97, "Shift", 12.25, r4, 1.75),
        k(98, "Up", 14.25, r4 + 0.25, 1.0),
        k(99, "End", 15.25, r4, 1.0),
        // Row 5
        k(105, "Ctrl", 0.0, r5, 1.25),
        k(106, "Win", 1.25, r5, 1.25),
        k(107, "Alt", 2.5, r5, 1.25),
        k(111, "Space", 3.75, r5, 6.25),
        k(115, "Alt", 10.0, r5, 1.0),
        k(116, "Fn", 11.0, r5, 1.0),
        k(117, "Menu", 12.0, r5, 1.0),
        k(119, "Left", 13.25, r5 + 0.25, 1.0),
        k(120, "Down", 14.25, r5 + 0.25, 1.0),
        k(121, "Right", 15.25, r5 + 0.25, 1.0),
    ];
    match model {
        Model::A75Master => {
            v.push(k(15, "F13", 14.0, r0, 1.0));
            v.push(k(14, "Del", 15.25, r1, 1.0));
        }
        Model::A75Ultra => {
            v.push(k(14, "Del", 14.0, r0, 1.0));
            v.push(k(36, "Home", 15.25, r1, 1.0));
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{Keymap, Layer};

    /// Identity reply captured from the user's board (debug.log 2026-11-07).
    pub const USER_IDENTITY: &str = "a0 02 00 01 0b 05 04 55 00 00 00 00 00 00 01 01 01 00 00 01 00 00 12 06 09 00 00 00 00 01 00 02 00 02 02 03 04 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00";

    pub fn hex(s: &str) -> Vec<u8> {
        s.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap()).collect()
    }

    #[test]
    fn parses_user_identity_as_master_fw85() {
        let id = Identity::parse(&hex(USER_IDENTITY)).unwrap();
        assert_eq!(id.model, Some(Model::A75Master));
        assert_eq!(id.firmware, 85);
        assert_eq!(id.firmware_label, "0.085-2.3.4");
        assert!(id.has_onboard_profiles());
        assert!(!id.upgrade_recommended());
        assert!(id.turbo && id.rapid_trigger && id.last_win && !id.rdt);
        assert_eq!((id.light_mode, id.light_speed, id.light_brightness), (0x12, 6, 9));
    }

    #[test]
    fn rejects_non_identity() {
        assert!(Identity::parse(&hex("fc 0b 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00")).is_none());
    }

    #[test]
    fn layouts_have_82_unique_slots_matching_factory_keymap() {
        for model in [Model::A75Master, Model::A75Ultra] {
            let l = model.layout();
            assert_eq!(l.len(), 82, "{model:?}");
            let mut slots: Vec<u8> = l.iter().map(|k| k.slot).collect();
            slots.sort();
            slots.dedup();
            assert_eq!(slots.len(), 82, "{model:?} duplicate slot");
            // Every drawn key must have a factory default-layer action.
            let km = Keymap::factory(model);
            for key in &l {
                assert!(!km.get(Layer::Default, key.slot as usize).is_none(), "{model:?} slot {} ({}) has no factory action", key.slot, key.label);
            }
        }
    }
}
