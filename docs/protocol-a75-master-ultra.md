# Official driver RE — A75 Master / Ultra (beta.drunkdeer-antler.com)

Source: `index.exKUY8vl.js` saved 2026-11-07 (bundle kept out of git under `tools/captures/antler-beta/`).
Prettified with prettier 3.3.3 (`--parser babel --print-width 140`) -> ~58.7k lines. Line refs below = prettified file.
Data extracted: `docs/data/a75master-keymap.json`, `a75ultra-keymap.json`, `a75master-ui-layout.json`.
Extract script: `tools/antler-extract/extract_keymap_layers.py <pretty.js> <start> <end> <out.json>`.

## Model dispatch (`xC`, ~L58040)
- Identity reply type bytes `[4..6]`: `0b 04 04` -> Ultra (756, classes ls/us/ks); `0b 05 04` -> Master (757, hs/ds/Os).
- Firmware version = DECIMAL string concat of two bytes: `Number(C.toString() + B.toString())`. User board `... 55 00` -> v85 (not hex 0x0055).
- Both Ultra + Master: `setPrecision(2)`, `oldOpenHighPrecision` stays false -> NEW high-precision (0xFD packets).
- `Tg = {v34, v63, v81}`: fw <= 34 -> `is_m_u_l` (legacy mode); 63 < fw < 81 -> prompts firmware upgrade; fw >= 81 -> `event_send_profile_len` (firmware-side profiles).
- User board: Master, fw 85 -> newest path.

## Slot indices (UI layout `getA75Master()`, ~L6410)
`value` = slot index used by BOTH actuation arrays and remap. `m_kbdLayout` names (KP7/KP8/ARR_DW/CTRL_R...) are legacy labels; app upstream built its layout from those names -> wrong positions.

| Key | Master slot | Upstream app slot |
|---|---|---|
| Star (sends Esc) | 0 | 0 |
| F13 | 15 | missing |
| Delete | 14 | 14 (drawn top-right) |
| Home | none (Ultra: 36) | 35 |
| PgUp | 57 | 56 |
| PgDn | 78 | 78 |
| End | 99 | 99 |
| Fn / Menu(Fn2) | 116 / 117 | 116 / 117 |
| Up / Left / Down / Right | 98 / 119 / 120 / 121 | 98 / 118 / 119 / 120 |

Master vs Ultra keymap differ only at slot 15 (Master F13 0x68) and 36 (Ultra Home 0x4A).

## Remap (`sendRemapKeyData`, ~L52498)
- Packet `[A0 02 04 pkt(1..14) 0E group(1/2/3) | 9 entries x 6 bytes | ... [60]=A5]`, 63 bytes.
- Fixed 6-byte stride. `keyCmd==0` -> all 6 bytes zero (slot byte NOT written).
- Entry by keyType: 0 `[slot,cmd,0,code,0,0]`; 1/2 `[slot,cmd,code,0,0,0]`; 3 `[slot,cmd,0,0,0,0]`; 4 `[slot,F8,rtp,group,pos,rapidTriggerMode]`.
- `remapSaveToKeyboard`: 14 pkts default layer, 14 Fn1, 14 Fn2 — each from the full per-layer profile (official defaults + user edits).
- Default layer: cmd 0xFC. Modifiers Shift/Ctrl/Alt = type 1 masks (LCtrl 01, LShift 02, LAlt 04, RShift 20, RAlt 40); Win = type 0 HID 0xE3. Fn = cmd 0xFF type 3; Fn2 (Menu key) = cmd 0xFE type 3.
- Fn1 layer: slot 106 Win -> cmd 0xFB type 2 code 16 (Win lock).
- Fn2 layer: device functions — reset (slot 0, 0xFD/5), RT-match modes, RDT/LW/Turbo/RT toggles, hardcore/competitive/standard, LED loop/brightness/mode (0xFB). Full list in keymap JSON.

## Actuation (precision 2)
- `sendActionPointData` `[FD 01 pkt(0..4) | 2 bytes LE per key]`, 30 keys/pkt (pkt 4: 6 keys), value = clamp(0.2..3.3 mm) x 200.
- `sendDownStokeData` `[FD 04 ...]`, `sendUpStrokeData` `[FD 05 ...]`: clamp(0..3.1 mm) x 200.
- Read-back: `FD 07 01` AP, `FD 07 03` DS, `FD 07 04` US. LW `FC 05`, RDT `FC 07`.

## Other packets
- `sendClearRTPData` = `AA 00 01` (official, not reset). `sendResetKeyboardData` = `AA 00 00` (built but never sent by `resetKeyboard()`).
- RT mode `FD 0C mode`; LW replace `FC 0B v`; tracking `FD 03 0/1`.
- LW pairs `FC 01 00 n [main trigger 0 0]...`; RDT pairs `FC 03 n [main trigger actLo actHi rstLo rstHi 0 0]...` (act clamp 10..50, reset 300..500, x200 units).
- Firmware profiles: change index `FB 05 idx [active ids...]`; delete `AB idx`.

## LW / RDT / RT Match / slots (impl: `dd-proto/src/rtp.rs`)
- Save (`highRtpSaveToKeyboard`, types 603/756/757): `FC 0A mode` (1 LW, 2 RDT, 3 both, 0) then the table; only when RT on. Called after `apiSaveProfileSolution` (common + travel).
- Pull LW `FC 05` -> `FC 06`: count byte 2, 4-byte entries from 4, keys at +1/+2, both directions reported (dedup).
- Pull RDT `FC 07` -> `FC 08`: count byte 3, 8-byte entries from 4, keys +1/+2, u16 at +3/+5 (disjoint ranges: smaller = trigger 0.05..0.25, larger = reset 1.5..2.5). Order inferred.
- UI rules (official): LW/RDT exclusive; enabling forces RT on + Turbo off; "Turbo mode cannot be used under LW and RDT functions".
- RT Match: common byte 11 (identity 30). Auto mode `FD 0C id` (0 Standard, 1 Competitive, 2 Extreme/"hardcore", 255 none; identity 31). Turning RT Match off also sends 255.
- LW full replace `FC 0B v` (identity 32).
- Slots (fw >= 81): `A2 0` query -> `A2 n`; `A2 n` sets count; `FB 05 idx 0..n-1` switch; identity byte 20 = current index (0-based). Max 5.
- Knob `C1 03 event? sub? 0 [cmd][code by keyType: t0 ->[7], t1/t2 ->[6], t3 none]`.
- Firmware upgrade `F9 ...` (do not implement).

## Upstream app defects vs this spec (A75 Master, from user debug.log 2026-11-07)
Every push rewrites the full 3-layer keymap from the wrong layout:
- F13 (15) blank -> dead. PgUp written to 56, 57 blank -> PgUp dead. Arrows shifted: Left key gets Down, Down gets Right, Right blank.
- Shift/Ctrl/Alt sent as type-0 HID 0xE0-0xE6 instead of type-1 masks.
- Fn (116) / Fn2 (117) blank -> Fn layers unreachable. Fn1/Fn2 layers sent all-blank -> Win lock, LED keys, RT/LW/RDT toggles wiped.
- User remap targeted slot 35 ("Home"): no key on Master -> no effect.
- Actuation sent as `B6` OldHighPrec 1-byte; official uses `FD` 2-byte x200 on these boards.

## Lighting (re-extracted 2026-11-08 from "New folder (21).zip", same bundle; refs = prettier 3.3.3 width 140)
- Effect: `sendLedModeData` (L52534) = `AE 01 00 00 code speed bright colour` (63 B). Called by `event_set_led_mode(0, code, speed, bright, colour)` (L57603) from `Ig.apiSaveColourSolution` (L41083); high-precision path (Master/Ultra) uses table `code`.
- Effect table: Master class `Os` (L50413, list L50418-50671); Ultra `ks` (L49340) identical. Codes: Off 240, Spectrum 0, Always on 1 (preset), Surf right 2, Breath 3 (preset+colourful), Serpentine 4, Light by press 5 (both), Rotate marquee 6, Center surf 7, Fountain 8, Laser key 9 (both), Cross surf 10, Traffic 11, Glowing fish 12, Ripple 13 (both), Dance of demons 14, Snake 15, Random fountain 16, Heart 17; Turbo light / Custom light 18 (not sent via effect packet for Master).
- Colour byte: UI `k` = index+1 into default RGB list (ff0000, 00ff00, 0000ff, ffff00, ff00ff, 00ffff, ffffff) -> 1..7; colourful -> 0 (L36398-36404, Ig L41083). Brightness / speed sliders 0..9 (L36449).
- Custom light (per key): `transmit_color_report_packet` (L57628) -> 7 packets (6 for G60/G65) via `sendTurboLedModeData` (L52538): `AE 01 turbo 00 mode 06 bright FF` + 13 x `[0x80+slot, R, G, B]` + `FF` terminator. High-precision: mode = 19 (L36271); turbo 0 = Custom light, 1 = Turbo light. Entries = every layout key in layout order (no colour -> 0,0,0) (`buildPkt_custom_led_mode_select` L57608). Selecting Custom light sends only these (L36343-36362).
- Reply: keyboard answers `AE 01 ..`; handler `vC` is a no-op (L58410): lighting is NOT readable -> app keeps its copy (`store.lighting`).
- Selecting a non-turbo effect in the official UI also turns Turbo mode off (L36352-36366); Kraken does not (shows a note instead).

