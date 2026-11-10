//! Remap picker catalog and action labels.
//!
//! Items mirror the official driver's `initializeFuncArea` command groups
//! (labels translated). Modifiers use the encodings of the official default
//! layer: Ctrl/Shift/Alt as keyType-1 masks, Win as HID 0xE3/0xE7.

use crate::keymap::{cmd, KeyAction};

#[derive(Clone, Copy, Debug)]
pub struct Item {
    pub label: &'static str,
    pub action: KeyAction,
}

#[derive(Clone, Copy, Debug)]
pub struct Category {
    pub name: &'static str,
    pub items: &'static [Item],
}

const fn h(label: &'static str, usage: u8) -> Item {
    Item { label, action: KeyAction::hid(usage) }
}

const fn a(label: &'static str, c: u8, kind: u8, code: u8) -> Item {
    Item { label, action: KeyAction::new(c, kind, code) }
}

pub const CATEGORIES: &[Category] = &[
    Category {
        name: "Letters",
        items: &[
            h("A", 4), h("B", 5), h("C", 6), h("D", 7), h("E", 8), h("F", 9), h("G", 10), h("H", 11), h("I", 12),
            h("J", 13), h("K", 14), h("L", 15), h("M", 16), h("N", 17), h("O", 18), h("P", 19), h("Q", 20), h("R", 21),
            h("S", 22), h("T", 23), h("U", 24), h("V", 25), h("W", 26), h("X", 27), h("Y", 28), h("Z", 29),
        ],
    },
    Category {
        name: "Numbers & symbols",
        items: &[
            h("1", 30), h("2", 31), h("3", 32), h("4", 33), h("5", 34), h("6", 35), h("7", 36), h("8", 37), h("9", 38),
            h("0", 39), h("-", 45), h("=", 46), h("[", 47), h("]", 48), h("\\", 49), h(";", 51), h("'", 52), h("`", 53),
            h(",", 54), h(".", 55), h("/", 56),
        ],
    },
    Category {
        name: "Function & navigation",
        items: &[
            h("F1", 58), h("F2", 59), h("F3", 60), h("F4", 61), h("F5", 62), h("F6", 63), h("F7", 64), h("F8", 65),
            h("F9", 66), h("F10", 67), h("F11", 68), h("F12", 69), h("F13", 104), h("Esc", 41), h("Tab", 43),
            h("Caps", 57), h("Backspace", 42), h("Enter", 40), h("Space", 44), h("Ins", 73), h("Del", 76),
            h("Home", 74), h("End", 77), h("PgUp", 75), h("PgDn", 78), h("Up", 82), h("Down", 81), h("Left", 80),
            h("Right", 79), h("PrtSc", 70), h("ScrLk", 71), h("Pause", 72), h("Menu", 101),
        ],
    },
    Category {
        name: "Modifiers & layers",
        items: &[
            a("L Ctrl", cmd::KEY, 1, 0x01), a("L Shift", cmd::KEY, 1, 0x02), a("L Alt", cmd::KEY, 1, 0x04),
            h("L Win", 0xE3), a("R Ctrl", cmd::KEY, 1, 0x10), a("R Shift", cmd::KEY, 1, 0x20),
            a("R Alt", cmd::KEY, 1, 0x40), h("R Win", 0xE7), a("Fn", cmd::FN, 3, 0), a("Fn2", cmd::FN2, 3, 0),
        ],
    },
    Category {
        name: "Numpad",
        items: &[
            h("N1", 89), h("N2", 90), h("N3", 91), h("N4", 92), h("N5", 93), h("N6", 94), h("N7", 95), h("N8", 96),
            h("N9", 97), h("N0", 98), h("N+", 87), h("N-", 86), h("N*", 85), h("N/", 84), h("N.", 99),
            h("NEnter", 88), h("NumLk", 83),
        ],
    },
    Category {
        name: "Media & mouse",
        items: &[
            a("Vol+", 144, 3, 144), a("Vol-", 145, 3, 145), a("Mute", 146, 3, 146), a("Play/Pause", 147, 3, 147),
            a("Stop", 148, 3, 148), a("Prev", 149, 3, 149), a("Next", 150, 3, 150), a("Mouse L", 176, 3, 176),
            a("Mouse R", 177, 3, 177), a("Mouse M", 178, 3, 178), a("Wheel up", 181, 3, 181),
            a("Wheel down", 182, 3, 182), a("Wheel left", 183, 3, 183), a("Wheel right", 184, 3, 184),
        ],
    },
    Category {
        name: "Keyboard functions",
        items: &[
            a("Light on/off", cmd::DEVICE, 2, 14), a("Effect cycle", cmd::DEVICE, 2, 0), a("Color cycle", cmd::DEVICE, 2, 9),
            a("Effect pause", cmd::DEVICE, 2, 13), a("Prev effect", cmd::DEVICE, 2, 2), a("Next effect", cmd::DEVICE, 2, 1),
            a("Bright+", cmd::DEVICE, 2, 4), a("Bright-", cmd::DEVICE, 2, 5), a("Speed+", cmd::DEVICE, 2, 7),
            a("Speed-", cmd::DEVICE, 2, 8), a("Win lock", cmd::DEVICE, 2, 16), a("RDT toggle", cmd::DEVICE, 2, 78),
            a("LW toggle", cmd::DEVICE, 2, 79), a("RT toggle", cmd::PERF, 2, 32), a("RT Match", cmd::PERF, 2, 14),
            a("Extreme mode", cmd::PERF, 2, 48), a("Competitive", cmd::PERF, 2, 49), a("Standard mode", cmd::PERF, 2, 50),
        ],
    },
];

/// Functions that appear in factory layers but not in the picker groups.
const EXTRA: &[Item] = &[
    a("Reset", cmd::PERF, 2, 5),
    a("RDT", cmd::PERF, 2, 6),
    a("LW", cmd::PERF, 2, 7),
    a("Turbo", cmd::PERF, 2, 8),
    a("RT Match 1", cmd::PERF, 2, 9),
    a("RT Match 2", cmd::PERF, 2, 10),
    a("RT Match 3", cmd::PERF, 2, 11),
    a("RT Match 4", cmd::PERF, 2, 12),
    a("RT Match 5", cmd::PERF, 2, 13),
    a("Fn", cmd::FN, 1, 128),
    a("Fn2", cmd::FN2, 1, 128),
    a("Ctrl", cmd::KEY, 1, 0x01),
    a("Shift", cmd::KEY, 1, 0x02),
    a("Alt", cmd::KEY, 1, 0x04),
    a("Shift", cmd::KEY, 1, 0x20),
    a("Alt", cmd::KEY, 1, 0x40),
];

/// Short label for an action (keycaps, picker highlight).
pub fn label(action: KeyAction) -> String {
    if action.is_none() {
        return String::new();
    }
    for c in CATEGORIES {
        if let Some(i) = c.items.iter().find(|i| i.action == action) {
            return i.label.to_string();
        }
    }
    // Same command regardless of kind/code for layer keys and media.
    match action.cmd {
        cmd::FN => return "Fn".into(),
        cmd::FN2 => return "Fn2".into(),
        _ => {}
    }
    if let Some(i) = EXTRA.iter().find(|i| i.action == action) {
        return i.label.to_string();
    }
    if action.cmd == cmd::KEY && action.kind == 0 {
        return format!("HID {:02X}", action.code);
    }
    format!("{:02X}/{}/{:02X}", action.cmd, action.kind, action.code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::{Keymap, Layer, SLOTS};
    use crate::model::Model;

    #[test]
    fn every_factory_action_has_a_readable_label() {
        for model in [Model::A75Master, Model::A75Ultra] {
            let km = Keymap::factory(model);
            for layer in Layer::ALL {
                for slot in 0..SLOTS {
                    let a = km.get(layer, slot);
                    if a.is_none() || (a.cmd == cmd::KEY && a.kind == 0 && a.code == 0) {
                        continue;
                    }
                    let l = label(a);
                    assert!(!l.contains('/') || l.len() == 1, "{model:?} {layer:?} slot {slot}: unlabeled {a:?} -> {l}");
                }
            }
        }
    }

    #[test]
    fn picker_items_unique_per_category() {
        for c in CATEGORIES {
            for (i, x) in c.items.iter().enumerate() {
                assert!(!c.items[i + 1..].iter().any(|y| y.action == x.action), "{} duplicate {}", c.name, x.label);
            }
        }
    }
}
