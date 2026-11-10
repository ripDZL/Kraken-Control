//! Last Win (LW), Release Dual-Trigger (RDT), RT Match and onboard profile
//! slots — high-precision boards (Master/Ultra).
//!
//! Official save flow (`highRtpSaveToKeyboard`, used when the keyboard type is
//! 603/756/757): `FC 0A mode`, then `FC 01` (LW pairs) or `FC 03` (RDT pairs).
//! No remap or RTPAuthority packets are involved on these boards.

use serde::{Deserialize, Serialize};

use crate::packets::{Packet, PAYLOAD_LEN};

fn packet(head: &[u8]) -> Packet {
    let mut p = [0u8; PAYLOAD_LEN];
    p[..head.len()].copy_from_slice(head);
    p
}

/// Max pairs that fit one packet.
pub const MAX_LW_PAIRS: usize = 14;
pub const MAX_RDT_PAIRS: usize = 7;

/// Release Dual-Trigger pair. Distances are the wire values (mm), i.e. what
/// the official driver sends after its upstroke adjustment.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct RdtPair {
    /// Key whose press/release drives the pair.
    pub main: u8,
    /// Key emitted on release.
    pub trigger: u8,
    /// 0.05..=0.25 mm.
    pub active_mm: f32,
    /// 1.50..=2.50 mm.
    pub reset_mm: f32,
}

impl RdtPair {
    pub const ACTIVE_RANGE: (f32, f32) = (0.05, 0.25);
    pub const RESET_RANGE: (f32, f32) = (1.5, 2.5);

    pub fn new(main: u8, trigger: u8) -> Self {
        Self { main, trigger, active_mm: 0.05, reset_mm: 1.5 }
    }
}

/// `sendClearRtpData`: `FC 0A mode`; mode = 1 LW, 2 RDT, 3 both, 0 none.
pub fn clear(last_win: bool, rdt: bool) -> Packet {
    let mode = match (last_win, rdt) {
        (true, true) => 3,
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 0,
    };
    packet(&[0xFC, 0x0A, mode])
}

/// `sendCreateLwData`: `FC 01 00 n | main trigger 00 00 | ...`.
pub fn lw_pairs(pairs: &[(u8, u8)]) -> Packet {
    let pairs = &pairs[..pairs.len().min(MAX_LW_PAIRS)];
    let mut p = packet(&[0xFC, 0x01, 0x00, pairs.len() as u8]);
    for (i, (m, t)) in pairs.iter().enumerate() {
        p[4 + 4 * i] = *m;
        p[5 + 4 * i] = *t;
    }
    p
}

/// `sendCreateRdtData`: `FC 03 n | main trigger act(u16) reset(u16) 00 00 | ...`,
/// values x200, active clamped 10..=50, reset 300..=500.
pub fn rdt_pairs(pairs: &[RdtPair]) -> Packet {
    let pairs = &pairs[..pairs.len().min(MAX_RDT_PAIRS)];
    let mut p = packet(&[0xFC, 0x03, pairs.len() as u8]);
    for (i, r) in pairs.iter().enumerate() {
        let o = 3 + 8 * i;
        let act = ((r.active_mm * 200.0).round() as i32).clamp(10, 50) as u16;
        let rst = ((r.reset_mm * 200.0).round() as i32).clamp(300, 500) as u16;
        p[o] = r.main;
        p[o + 1] = r.trigger;
        p[o + 2..o + 4].copy_from_slice(&act.to_le_bytes());
        p[o + 4..o + 6].copy_from_slice(&rst.to_le_bytes());
    }
    p
}

/// `sendPullLw`: `FC 05`, reply `FC 06`.
pub fn pull_lw() -> Packet {
    packet(&[0xFC, 0x05])
}

/// `sendPullRdt`: `FC 07`, reply `FC 08`.
pub fn pull_rdt() -> Packet {
    packet(&[0xFC, 0x07])
}

/// Parses `FC 06` like `watch_lw_rdt("lw")`: count at byte 2, 4-byte
/// entries from byte 4 with the keys at +1/+2; reversed duplicates are
/// dropped (the firmware reports both directions).
pub fn parse_lw(r: &[u8]) -> Option<Vec<(u8, u8)>> {
    if r.len() < 4 || r[0] != 0xFC || r[1] != 0x06 {
        return None;
    }
    let n = r[2] as usize;
    let mut out: Vec<(u8, u8)> = Vec::new();
    let mut e = 4;
    while e < (n + 1) * 4 && e + 2 < r.len() {
        let (m, t) = (r[e + 1], r[e + 2]);
        if !out.iter().any(|&(a, b)| (a, b) == (m, t) || (a, b) == (t, m)) {
            out.push((m, t));
        }
        e += 4;
    }
    Some(out)
}

/// Parses `FC 08` like `watch_lw_rdt("rdt")`: count at byte 3, 8-byte
/// entries from byte 4 (keys at +1/+2, two u16 distances at +3/+5). The two
/// distances have disjoint ranges, so the smaller one is the active point.
pub fn parse_rdt(r: &[u8]) -> Option<Vec<RdtPair>> {
    if r.len() < 4 || r[0] != 0xFC || r[1] != 0x08 {
        return None;
    }
    let n = r[3] as usize;
    let mut out = Vec::new();
    let mut e = 4;
    while e < n * 8 && e + 6 < r.len() {
        let a = u16::from_le_bytes([r[e + 3], r[e + 4]]) as f32 / 200.0;
        let b = u16::from_le_bytes([r[e + 5], r[e + 6]]) as f32 / 200.0;
        out.push(RdtPair { main: r[e + 1], trigger: r[e + 2], active_mm: a.min(b), reset_mm: a.max(b) });
        e += 8;
    }
    Some(out)
}

/// RT Match auto modes (`sendRtModeDate`, `FD 0C id`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchMode {
    Standard = 0,
    Competitive = 1,
    Extreme = 2,
}

impl MatchMode {
    pub const ALL: [MatchMode; 3] = [MatchMode::Standard, MatchMode::Competitive, MatchMode::Extreme];
    /// Wire value for "no auto mode".
    pub const OFF: u8 = 255;

    pub fn from_wire(v: u8) -> Option<MatchMode> {
        match v {
            0 => Some(MatchMode::Standard),
            1 => Some(MatchMode::Competitive),
            2 => Some(MatchMode::Extreme),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            MatchMode::Standard => "Standard",
            MatchMode::Competitive => "Competitive",
            MatchMode::Extreme => "Extreme",
        }
    }

    pub fn help(self) -> &'static str {
        match self {
            MatchMode::Standard => "Adaptive for office work and gaming.",
            MatchMode::Competitive => "Balances sensitivity and stability at speed.",
            MatchMode::Extreme => "Highest RT accuracy, fastest response.",
        }
    }
}

pub fn match_mode(mode: Option<MatchMode>) -> Packet {
    packet(&[0xFD, 0x0C, mode.map_or(MatchMode::OFF, |m| m as u8)])
}

/// Onboard profile slots (firmware >= 81). `sendProfileLen`: `A2 n`;
/// n = 0 queries, the reply `A2 n` carries the current slot count.
pub fn slot_count(n: u8) -> Packet {
    packet(&[0xA2, n])
}

pub fn parse_slot_count(r: &[u8]) -> Option<u8> {
    (r.len() >= 2 && r[0] == 0xA2).then_some(r[1])
}

/// `sendChangeProfileIndex`: `FB 05 index | ids of active profiles`.
pub fn select_slot(index: u8, count: u8) -> Packet {
    let mut p = packet(&[0xFB, 0x05, index]);
    for i in 0..count.min(5) {
        p[3 + i as usize] = i;
    }
    p
}

pub const MAX_SLOTS: u8 = 5;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lw_packet_layout() {
        let p = lw_pairs(&[(64, 66), (44, 65)]);
        assert_eq!(&p[..12], &[0xFC, 0x01, 0x00, 2, 64, 66, 0, 0, 44, 65, 0, 0]);
        assert!(p[12..].iter().all(|&b| b == 0));
    }

    #[test]
    fn rdt_packet_layout_and_clamps() {
        let p = rdt_pairs(&[RdtPair { main: 44, trigger: 65, active_mm: 0.01, reset_mm: 9.0 }]);
        assert_eq!(&p[..3], &[0xFC, 0x03, 1]);
        assert_eq!(&p[3..11], &[44, 65, 10, 0, 0xF4, 0x01, 0, 0]); // 10, 500
    }

    #[test]
    fn clear_modes() {
        assert_eq!(&clear(true, false)[..3], &[0xFC, 0x0A, 1]);
        assert_eq!(&clear(false, true)[..3], &[0xFC, 0x0A, 2]);
        assert_eq!(&clear(true, true)[..3], &[0xFC, 0x0A, 3]);
        assert_eq!(&clear(false, false)[..3], &[0xFC, 0x0A, 0]);
    }

    #[test]
    fn parse_lw_dedups_reversed_pairs() {
        // count 4: (64,66) (66,64) (44,65) (65,44)
        let mut r = vec![0u8; 63];
        r[..3].copy_from_slice(&[0xFC, 0x06, 4]);
        for (i, (m, t)) in [(64u8, 66u8), (66, 64), (44, 65), (65, 44)].iter().enumerate() {
            r[4 + 4 * i + 1] = *m;
            r[4 + 4 * i + 2] = *t;
        }
        assert_eq!(parse_lw(&r).unwrap(), vec![(64, 66), (44, 65)]);
        assert!(parse_lw(&[0xFC, 0x08, 0, 0]).is_none());
    }

    #[test]
    fn parse_rdt_orders_distances_by_range() {
        let mut r = vec![0u8; 63];
        r[..4].copy_from_slice(&[0xFC, 0x08, 0, 1]);
        r[5] = 44;
        r[6] = 65;
        r[7..9].copy_from_slice(&300u16.to_le_bytes());
        r[9..11].copy_from_slice(&20u16.to_le_bytes());
        let v = parse_rdt(&r).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!((v[0].main, v[0].trigger), (44, 65));
        assert!((v[0].active_mm - 0.1).abs() < 1e-6 && (v[0].reset_mm - 1.5).abs() < 1e-6);
    }

    #[test]
    fn match_mode_and_slots() {
        assert_eq!(&match_mode(Some(MatchMode::Extreme))[..3], &[0xFD, 0x0C, 2]);
        assert_eq!(&match_mode(None)[..3], &[0xFD, 0x0C, 255]);
        assert_eq!(&select_slot(1, 3)[..6], &[0xFB, 0x05, 1, 0, 1, 2]);
        assert_eq!(parse_slot_count(&[0xA2, 3]), Some(3));
    }
}
