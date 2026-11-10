//! Packet builders and response parsers (63-byte payloads).
//!
//! Transport framing: HID output report ID 4, payload = these 63 bytes.
//! The keyboard answers most commands with a report whose first byte equals
//! the command's first byte; see [`expected_reply`].

use crate::keymap::{KeyAction, Keymap, Layer, SLOTS};

pub const REPORT_ID: u8 = 0x04;
pub const PAYLOAD_LEN: usize = 63;

pub type Packet = [u8; PAYLOAD_LEN];

fn packet(head: &[u8]) -> Packet {
    let mut p = [0u8; PAYLOAD_LEN];
    p[..head.len()].copy_from_slice(head);
    p
}

/// `sendIdentityData`: `A0 02 00 ...`.
pub fn identity() -> Packet {
    packet(&[0xA0, 0x02, 0x00])
}

/// Global switches carried by the common packet (`sendCommonData`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommonFlags {
    pub turbo: bool,
    pub rapid_trigger: bool,
    pub last_win: bool,
    pub rdt: bool,
    pub rt_match: bool,
}

/// `sendCommonData`: `B5 00 1E 01 00 00 01 turbo rt 00 lwrdt rtmatch`.
pub fn common(f: CommonFlags) -> Packet {
    let lwrdt = match (f.last_win, f.rdt) {
        (true, true) => 3,
        (true, false) => 1,
        (false, true) => 2,
        (false, false) => 0,
    };
    packet(&[0xB5, 0x00, 0x1E, 0x01, 0x00, 0x00, 0x01, f.turbo as u8, f.rapid_trigger as u8, 0x00, lwrdt, f.rt_match as u8])
}

/// Per-key travel value kinds (high-precision dialect, `0xFD` family).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Travel {
    /// Actuation point.
    Ap,
    /// Rapid-trigger downstroke sensitivity.
    Ds,
    /// Rapid-trigger upstroke sensitivity.
    Us,
}

impl Travel {
    pub const ALL: [Travel; 3] = [Travel::Ap, Travel::Ds, Travel::Us];

    fn write_sub(self) -> u8 {
        match self {
            Travel::Ap => 0x01,
            Travel::Ds => 0x04,
            Travel::Us => 0x05,
        }
    }

    fn pull_sub(self) -> u8 {
        match self {
            Travel::Ap => 0x01,
            Travel::Ds => 0x03,
            Travel::Us => 0x04,
        }
    }

    /// Second byte of the read-back reply (`FD 08`, `FD 0A`, `FD 0B`).
    pub fn reply_sub(self) -> u8 {
        match self {
            Travel::Ap => 0x08,
            Travel::Ds => 0x0A,
            Travel::Us => 0x0B,
        }
    }

    /// Clamp range in mm (official constants `Ie`, `Ng`, `Yg`).
    pub fn range_mm(self) -> (f32, f32) {
        match self {
            Travel::Ap => (0.2, 3.3),
            Travel::Ds | Travel::Us => (0.0, 3.1),
        }
    }
}

/// Wire unit: 1/200 mm.
pub const UNITS_PER_MM: f32 = 200.0;

/// Converts mm to wire units, clamped like the official driver. Rounds to
/// nearest (official truncates float error, e.g. 0.29*200 -> 57).
pub fn mm_to_units(kind: Travel, mm: f32) -> u16 {
    let (lo, hi) = kind.range_mm();
    (mm.clamp(lo, hi) * UNITS_PER_MM).round() as u16
}

pub fn units_to_mm(u: u16) -> f32 {
    u as f32 / UNITS_PER_MM
}

/// `sendActionPointData` / `sendDownStokeData` / `sendUpStrokeData`:
/// 5 packets `FD sub idx | u16 LE x30` (packet 4 carries slots 120..125).
pub fn travel_write(kind: Travel, mm: &[f32; SLOTS]) -> Vec<Packet> {
    (0..5u8)
        .map(|idx| {
            let mut p = packet(&[0xFD, kind.write_sub(), idx]);
            let count = if idx < 4 { 30 } else { 6 };
            for r in 0..count {
                let v = mm_to_units(kind, mm[idx as usize * 30 + r]).to_le_bytes();
                p[3 + 2 * r] = v[0];
                p[4 + 2 * r] = v[1];
            }
            p
        })
        .collect()
}

/// `sendPull*Data`: `FD 07 sub`. Keyboard replies with 5 `FD reply_sub` packets.
pub fn travel_pull(kind: Travel) -> Packet {
    packet(&[0xFD, 0x07, kind.pull_sub()])
}

/// Accumulates read-back replies like the official `watch_ap_rt`: values
/// from byte 3, 30 per packet, appended in arrival order; complete at >125.
#[derive(Clone, Debug, Default)]
pub struct TravelReadback {
    values: Vec<u16>,
}

impl TravelReadback {
    /// Feeds one reply; returns false if it isn't a reply for `kind`.
    pub fn feed(&mut self, kind: Travel, reply: &[u8]) -> bool {
        if reply.len() < PAYLOAD_LEN || reply[0] != 0xFD || reply[1] != kind.reply_sub() {
            return false;
        }
        for pair in reply[3..PAYLOAD_LEN].as_chunks::<2>().0 {
            self.values.push(u16::from_le_bytes(*pair));
        }
        true
    }

    pub fn is_complete(&self) -> bool {
        self.values.len() > 125
    }

    pub fn mm(&self) -> Option<[f32; SLOTS]> {
        if !self.is_complete() {
            return None;
        }
        let mut out = [0f32; SLOTS];
        for (o, v) in out.iter_mut().zip(&self.values) {
            *o = units_to_mm(*v);
        }
        Some(out)
    }
}

/// `sendRemapKeyData` for one layer: 14 packets
/// `A0 02 04 pkt(1..14) 0E group | 9 x 6-byte entries | [60] = A5`.
pub fn remap_layer(layer: Layer, actions: &[KeyAction; SLOTS]) -> Vec<Packet> {
    (1..=14u8)
        .map(|pkt| {
            let mut p = packet(&[0xA0, 0x02, 0x04, pkt, 0x0E, layer.group()]);
            for w in 0..9 {
                let slot = (pkt as usize - 1) * 9 + w;
                if slot >= SLOTS {
                    break;
                }
                let r = 6 + w * 6;
                p[r..r + 6].copy_from_slice(&actions[slot].encode(slot as u8));
            }
            p[60] = 0xA5;
            p
        })
        .collect()
}

/// `remapSaveToKeyboard`: default, Fn, Fn2 layers in that order (42 packets).
pub fn remap_all(km: &Keymap) -> Vec<Packet> {
    Layer::ALL.iter().flat_map(|&l| remap_layer(l, &km.layers[l as usize])).collect()
}

/// `sendRtModeDate`: RT-match auto mode (`FD 0C mode`).
pub fn rt_match_mode(mode: u8) -> Packet {
    packet(&[0xFD, 0x0C, mode])
}

/// `sendLwReplaceData`: `FC 0B v`.
pub fn lw_replace(on: bool) -> Packet {
    packet(&[0xFC, 0x0B, on as u8])
}

/// Key-depth streaming (`sendTrackingStart/StopData`, high-precision dialect).
pub fn tracking(on: bool) -> Packet {
    packet(&[0xFD, 0x03, on as u8])
}

/// What reply completes a request, per the official input handler: most
/// commands are acknowledged by a report with the same first byte.
/// Returns the expected `(byte0, Option<byte1>)`.
pub fn expected_reply(req: &Packet) -> (u8, Option<u8>) {
    match (req[0], req[1]) {
        (0xA0, 0x02) => (0xA0, Some(0x02)),
        (0xFD, 0x07) => (0xFD, None),
        (0xFC, 0x05) => (0xFC, Some(0x06)),
        (0xFC, 0x07) => (0xFC, Some(0x08)),
        (b0, _) => (b0, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Model;

    fn hex(p: &[u8]) -> String {
        p.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn common_packet_layout() {
        let p = common(CommonFlags { turbo: false, rapid_trigger: true, last_win: true, rdt: false, rt_match: false });
        assert_eq!(&p[..12], &[0xB5, 0, 0x1E, 1, 0, 0, 1, 0, 1, 0, 1, 0]);
        assert!(p[12..].iter().all(|&b| b == 0));
    }

    #[test]
    fn travel_write_matches_official_encoding() {
        let mut ap = [2.0f32; SLOTS];
        ap[0] = 0.2;
        ap[29] = 3.3;
        ap[30] = 9.0; // clamped to 3.3
        ap[125] = 1.27;
        let pk = travel_write(Travel::Ap, &ap);
        assert_eq!(pk.len(), 5);
        assert_eq!(&pk[0][..5], &[0xFD, 0x01, 0x00, 40, 0]); // 0.2mm = 40
        assert_eq!(&pk[0][3 + 58..3 + 60], &660u16.to_le_bytes()); // slot 29 = 3.3mm
        assert_eq!(&pk[1][..5], &[0xFD, 0x01, 0x01, 0x94, 0x02]); // slot 30 clamped 660
        assert_eq!(pk[4][2], 4);
        assert_eq!(&pk[4][3 + 10..3 + 12], &254u16.to_le_bytes()); // slot 125 = 1.27mm
        assert!(pk[4][15..].iter().all(|&b| b == 0), "packet 4 carries only 6 slots");
    }

    #[test]
    fn travel_readback_roundtrip() {
        let mut mm = [0f32; SLOTS];
        for (i, v) in mm.iter_mut().enumerate() {
            *v = 0.01 * (i as f32 % 30.0);
        }
        let mut rb = TravelReadback::default();
        for p in travel_write(Travel::Ds, &mm) {
            let mut reply = p;
            reply[1] = Travel::Ds.reply_sub();
            assert!(rb.feed(Travel::Ds, &reply));
        }
        let back = rb.mm().unwrap();
        for i in 0..SLOTS {
            assert!((back[i] - mm[i]).abs() < 0.003, "slot {i}");
        }
        assert!(!rb.feed(Travel::Us, &travel_write(Travel::Ds, &mm)[0]));
    }

    /// Official Master defaults, encoded, vs. the entries the old C# app
    /// sent (user debug.log) — the 15 genuine differences must now match
    /// the official values.
    #[test]
    fn master_default_layer_encodes_like_official() {
        let km = Keymap::factory(Model::A75Master);
        let pk = remap_layer(Layer::Default, &km.layers[0]);
        assert_eq!(pk.len(), 14);
        let entry = |slot: usize| {
            let p = &pk[slot / 9];
            let r = 6 + (slot % 9) * 6;
            hex(&p[r..r + 6])
        };
        assert_eq!(&pk[0][..6], &[0xA0, 0x02, 0x04, 0x01, 0x0E, 0x01]);
        assert_eq!(pk[0][60], 0xA5);
        assert_eq!(entry(0), "00 fc 00 29 00 00"); // star key = Esc
        assert_eq!(entry(15), "0f fc 00 68 00 00"); // F13
        assert_eq!(entry(14), "0e fc 00 4c 00 00"); // Delete
        assert_eq!(entry(35), "00 00 00 00 00 00"); // no key on Master
        assert_eq!(entry(57), "39 fc 00 4b 00 00"); // PgUp
        assert_eq!(entry(84), "54 fc 02 00 00 00"); // LShift mask
        assert_eq!(entry(97), "61 fc 20 00 00 00"); // RShift mask
        assert_eq!(entry(105), "69 fc 01 00 00 00"); // LCtrl mask
        assert_eq!(entry(106), "6a fc 00 e3 00 00"); // Win as HID
        assert_eq!(entry(107), "6b fc 04 00 00 00"); // LAlt mask
        assert_eq!(entry(115), "73 fc 40 00 00 00"); // RAlt mask
        assert_eq!(entry(116), "74 ff 00 00 00 00"); // Fn
        assert_eq!(entry(117), "75 fe 00 00 00 00"); // Fn2
        assert_eq!(entry(118), "00 00 00 00 00 00");
        assert_eq!(entry(119), "77 fc 00 50 00 00"); // Left
        assert_eq!(entry(120), "78 fc 00 51 00 00"); // Down
        assert_eq!(entry(121), "79 fc 00 4f 00 00"); // Right
    }

    #[test]
    fn master_fn_layers_are_not_blank() {
        let km = Keymap::factory(Model::A75Master);
        let all = remap_all(&km);
        assert_eq!(all.len(), 42);
        assert_eq!(all[14][5], 2);
        assert_eq!(all[28][5], 3);
        // Fn layer: Win -> Win lock (FB, type 2, code 16) at slot 106.
        assert_eq!(km.get(Layer::Fn, 106), KeyAction::new(0xFB, 2, 16));
        let p = &all[14 + 106 / 9];
        let r = 6 + (106 % 9) * 6;
        assert_eq!(&p[r..r + 6], &[106, 0xFB, 16, 0, 0, 0]);
        // Fn2 layer carries 19 device functions on the Master.
        let n = km.layers[2].iter().filter(|a| !a.is_none()).count();
        assert_eq!(n, 19);
    }

    #[test]
    fn identity_and_expected_replies() {
        assert_eq!(&identity()[..3], &[0xA0, 0x02, 0x00]);
        assert_eq!(expected_reply(&identity()), (0xA0, Some(0x02)));
        assert_eq!(expected_reply(&travel_pull(Travel::Ap)), (0xFD, None));
        assert_eq!(expected_reply(&common(CommonFlags::default())), (0xB5, None));
        assert_eq!(expected_reply(&crate::rtp::pull_lw()), (0xFC, Some(0x06)));
        assert_eq!(expected_reply(&crate::rtp::pull_rdt()), (0xFC, Some(0x08)));
        assert_eq!(expected_reply(&crate::rtp::slot_count(0)), (0xA2, None));
    }
}
