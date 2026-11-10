//! Gen-1 (VID 0x352D: A75 Master / Ultra) lighting: firmware effect select and
//! per-key "Custom light". Facts from the official driver bundle
//! (`sendLedModeData`, `sendTurboLedModeData`, `transmit_color_report_packet`,
//! Master colour class `Os`); see docs/protocol-a75-master-ultra.md.
//! The keyboard only acknowledges (`AE 01 ..`); lighting can't be read back,
//! so the app keeps its own copy.

use crate::packets::{Packet, PAYLOAD_LEN};

pub const CMD: u8 = 0xAE;
/// Effect code that turns the backlight off.
pub const OFF: u8 = 0xF0;
/// Brightness and speed range: 0..=MAX_LEVEL.
pub const MAX_LEVEL: u8 = 9;
/// Mode byte of the per-key colour packets (high-precision firmware path).
pub const CUSTOM_MODE: u8 = 19;
/// Per-key colours always go out as this many packets.
pub const CUSTOM_PACKETS: usize = 7;
pub const KEYS_PER_PACKET: usize = 13;

/// Effect colour byte: 0 = colourful (rainbow), 1..=7 = `PRESETS[n - 1]`.
pub const COLOURFUL: u8 = 0;
pub const PRESETS: [(&str, [u8; 3]); 7] = [
    ("Red", [255, 0, 0]),
    ("Green", [0, 255, 0]),
    ("Blue", [0, 0, 255]),
    ("Yellow", [255, 255, 0]),
    ("Magenta", [255, 0, 255]),
    ("Cyan", [0, 255, 255]),
    ("White", [255, 255, 255]),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effect {
    pub name: &'static str,
    pub code: u8,
    /// Offers the 7 preset colours.
    pub preset: bool,
    /// Offers "colourful" (rainbow).
    pub colourful: bool,
}

const fn fx(name: &'static str, code: u8, preset: bool, colourful: bool) -> Effect {
    Effect { name, code, preset, colourful }
}

/// Firmware effects of the A75 Master / Ultra, in the official order.
/// "Custom light" (per-key) is separate: see `custom`.
pub const EFFECTS: &[Effect] = &[
    fx("Off", OFF, false, false),
    fx("Spectrum", 0, false, false),
    fx("Always on", 1, true, false),
    fx("Surf right", 2, false, false),
    fx("Breath", 3, true, true),
    fx("Serpentine", 4, false, false),
    fx("Light by press", 5, true, true),
    fx("Rotate marquee", 6, false, false),
    fx("Center surf", 7, false, false),
    fx("Colour fountain", 8, false, false),
    fx("Laser key", 9, true, true),
    fx("Cross surf", 10, false, false),
    fx("Traffic", 11, false, false),
    fx("Glowing fish", 12, false, false),
    fx("Ripple", 13, true, true),
    fx("Dance of demons", 14, false, false),
    fx("Snake", 15, false, false),
    fx("Random fountain", 16, false, false),
    fx("Heart", 17, false, false),
];

pub fn effect_by_code(code: u8) -> Option<&'static Effect> {
    EFFECTS.iter().find(|e| e.code == code)
}

/// Selects a firmware effect: `AE 01 00 00 code speed bright colour`.
pub fn effect(code: u8, speed: u8, bright: u8, colour: u8) -> Packet {
    let mut p = [0u8; PAYLOAD_LEN];
    p[..8].copy_from_slice(&[CMD, 0x01, 0x00, 0x00, code, speed.min(MAX_LEVEL), bright.min(MAX_LEVEL), colour]);
    p
}

/// "Custom light": per-key colours, which also selects the custom mode.
/// `keys` = (slot, rgb) in layout order; at most 7 × 13 are sent. Each packet:
/// `AE 01 00 00 13 06 bright FF` + `[0x80 | slot, r, g, b]` × n + `FF`.
pub fn custom(keys: &[(u8, [u8; 3])], bright: u8) -> Vec<Packet> {
    (0..CUSTOM_PACKETS)
        .map(|i| {
            let mut p = [0u8; PAYLOAD_LEN];
            p[..8].copy_from_slice(&[CMD, 0x01, 0x00, 0x00, CUSTOM_MODE, 0x06, bright.min(MAX_LEVEL), 0xFF]);
            let chunk = keys.iter().skip(i * KEYS_PER_PACKET).take(KEYS_PER_PACKET);
            let mut at = 8;
            for (slot, [r, g, b]) in chunk {
                p[at..at + 4].copy_from_slice(&[0x80 | slot, *r, *g, *b]);
                at += 4;
            }
            p[at] = 0xFF;
            p
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_packet() {
        let p = effect(3, 5, 9, 2);
        assert_eq!(&p[..8], &[0xAE, 0x01, 0x00, 0x00, 0x03, 0x05, 0x09, 0x02]);
        assert!(p[8..].iter().all(|b| *b == 0));
        assert_eq!(&effect(OFF, 20, 20, 0)[4..8], &[0xF0, 9, 9, 0]);
    }

    #[test]
    fn custom_packets() {
        let keys = [(0u8, [0x12, 0x34, 0x56]), (121, [1, 2, 3])];
        let p = custom(&keys, 7);
        assert_eq!(p.len(), CUSTOM_PACKETS);
        assert_eq!(&p[0][..17], &[0xAE, 0x01, 0, 0, 19, 6, 7, 0xFF, 0x80, 0x12, 0x34, 0x56, 0x80 + 121, 1, 2, 3, 0xFF]);
        assert!(p[0][17..].iter().all(|b| *b == 0));
        // Empty packets still carry the header and an immediate terminator.
        assert_eq!(&p[1][..9], &[0xAE, 0x01, 0, 0, 19, 6, 7, 0xFF, 0xFF]);
    }

    #[test]
    fn custom_chunks_13_keys() {
        let keys: Vec<(u8, [u8; 3])> = (0..30u8).map(|s| (s, [s, s, s])).collect();
        let p = custom(&keys, 9);
        assert_eq!(p[0][8 + 12 * 4], 0x80 + 12);
        assert_eq!(p[0][8 + 13 * 4], 0xFF);
        assert_eq!(p[1][8], 0x80 + 13);
        assert_eq!(p[2][8], 0x80 + 26);
        assert_eq!(p[2][8 + 4 * 4], 0xFF);
        assert_eq!(p[3][8], 0xFF);
        // 13 entries + terminator fit the 63-byte payload exactly at index 60.
        assert_eq!(8 + 13 * 4, 60);
    }

    #[test]
    fn table() {
        assert_eq!(EFFECTS.len(), 19);
        assert_eq!(effect_by_code(13).map(|e| e.name), Some("Ripple"));
        assert!(effect_by_code(18).is_none());
        let mut codes: Vec<u8> = EFFECTS.iter().map(|e| e.code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), EFFECTS.len());
    }
}
