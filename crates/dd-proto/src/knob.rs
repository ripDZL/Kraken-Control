//! Gen-1 knob (A75 Master / Ultra, firmware 0.063+).
//!
//! Official `sendKnobEventData` (index.exKUY8vl.js, char ~6073081):
//! `C1 03 event mapped 0 cmd [6] [7]`, the code placed by keyType like a remap
//! entry: keyType 0 -> byte 7, 1/2 -> byte 6, 3 -> no code. `mapped` 0 with no
//! action restores the knob's own behaviour for that event ("reset" sends all
//! six that way). Events in the official Knob component order (char ~5236000):
//! right, left, down, up, right_down, left_down. The keyboard acknowledges with
//! a `C1` report; the mapping can't be read back.

use crate::keymap::KeyAction;
use crate::packets::{Packet, PAYLOAD_LEN};

/// Knob shown by the official driver only for A75 Ultra / Master with fw >= 63.
pub const MIN_FIRMWARE: u32 = 63;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KnobEvent {
    RotateRight = 1,
    RotateLeft = 2,
    Press = 3,
    Lift = 4,
    PressRotateRight = 5,
    PressRotateLeft = 6,
}

impl KnobEvent {
    pub const ALL: [KnobEvent; 6] = [
        KnobEvent::RotateRight,
        KnobEvent::RotateLeft,
        KnobEvent::Press,
        KnobEvent::Lift,
        KnobEvent::PressRotateRight,
        KnobEvent::PressRotateLeft,
    ];

    /// Official English labels.
    pub fn name(self) -> &'static str {
        match self {
            KnobEvent::RotateRight => "Rotate Right",
            KnobEvent::RotateLeft => "Rotate Left",
            KnobEvent::Press => "Press",
            KnobEvent::Lift => "Lift",
            KnobEvent::PressRotateRight => "Press + Rotate Right",
            KnobEvent::PressRotateLeft => "Press + Rotate Left",
        }
    }

    pub fn index(self) -> usize {
        self as usize - 1
    }
}

/// What each event sends; `None` = the knob's own behaviour.
pub type KnobMap = [Option<KeyAction>; 6];

/// One knob event packet.
pub fn packet(event: KnobEvent, action: Option<KeyAction>) -> Packet {
    let mut p = [0u8; PAYLOAD_LEN];
    p[0] = 0xC1;
    p[1] = 0x03;
    p[2] = event as u8;
    let Some(a) = action.filter(|a| !a.is_none()) else { return p };
    p[3] = 1;
    p[5] = a.cmd;
    match a.kind {
        0 => p[7] = a.code,
        1 | 2 => p[6] = a.code,
        _ => {}
    }
    p
}

/// All six events, in the official save order.
pub fn packets(map: &KnobMap) -> Vec<Packet> {
    KnobEvent::ALL.iter().map(|&e| packet(e, map[e.index()])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hid_code_goes_to_byte_7() {
        let p = packet(KnobEvent::RotateRight, Some(KeyAction::hid(0x80)));
        assert_eq!(&p[..8], &[0xC1, 0x03, 0x01, 0x01, 0x00, KeyAction::hid(0x80).cmd, 0x00, 0x80]);
        assert!(p[8..].iter().all(|&b| b == 0));
    }

    #[test]
    fn modifier_and_function_codes_go_to_byte_6() {
        let p = packet(KnobEvent::Press, Some(KeyAction::new(0xFC, 1, 0x02)));
        assert_eq!(&p[..8], &[0xC1, 0x03, 0x03, 0x01, 0x00, 0xFC, 0x02, 0x00]);
        let p = packet(KnobEvent::Lift, Some(KeyAction::new(0xFB, 2, 0x10)));
        assert_eq!(&p[..8], &[0xC1, 0x03, 0x04, 0x01, 0x00, 0xFB, 0x10, 0x00]);
    }

    #[test]
    fn command_only_has_no_code() {
        let p = packet(KnobEvent::PressRotateLeft, Some(KeyAction::new(0xFF, 3, 0x55)));
        assert_eq!(&p[..8], &[0xC1, 0x03, 0x06, 0x01, 0x00, 0xFF, 0x00, 0x00]);
    }

    #[test]
    fn default_and_none_action_reset_the_event() {
        for p in [packet(KnobEvent::RotateLeft, None), packet(KnobEvent::RotateLeft, Some(KeyAction::NONE))] {
            assert_eq!(&p[..3], &[0xC1, 0x03, 0x02]);
            assert!(p[3..].iter().all(|&b| b == 0));
        }
    }

    #[test]
    fn packets_in_event_order() {
        let mut map: KnobMap = [None; 6];
        map[KnobEvent::Press.index()] = Some(KeyAction::hid(0x2C));
        let all = packets(&map);
        assert_eq!(all.iter().map(|p| p[2]).collect::<Vec<_>>(), vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(all[2][3], 1);
        assert_eq!(all[2][7], 0x2C);
        assert!(all.iter().enumerate().all(|(i, p)| i == 2 || p[3] == 0));
    }
}
