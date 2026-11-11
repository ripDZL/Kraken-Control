# Gen-2 protocol (VID 0x19F5) — 2025 A75 Ultra and siblings

Source: official web driver https://drunkdeer.keybord.net.cn (Next.js; `app/page-feab963f2440235f.js`,
downloaded 2026-11-07 into `Drunk Deer/gen2-bundle/`). Spec/data only; no code copied.
Impl: `crates/dd-proto/src/gen2.rs`, `app/src/device2.rs`, `app/src/ui2.rs`. Data: `crates/dd-proto/data/gen2-a75ultra.json`.

## Devices
- User board: `USB\VID_19F5&PID_FB3E` "A75Ultra" (3 interfaces). Official table `ea["0x19F5_0xFB3E"]`: type 101, layout `K75`, latest fw 121 (`A75Ultra_CS9AB11334_V121_20260415_1.bin`).
- Same driver: FB50 X60 Future, FB5E A75 Master (gen-2, layout K75M, type 104), FB5C A75 Pro (type 103), FB42 X60 HE; dongles AFxx (usage page FF00/1) take a different path.
- Gen-1 VID 0x352D boards are redirected to `/drunk/index.html` (old driver).
- HID filter: wired = usage page 0x0001, usage 0; dongle = 0xFF00 / 1. Devices whose productName matches `/wb/i` are skipped.

## Transport
- `sendReport(0, 64 bytes)` → report ID 0; Windows buffer 65 bytes `[0, payload]`.
- One command in flight (`commandQueue`), next input report = reply; official races 1.5 s and returns `[]` on timeout.
- Reply byte 0 `AA` = OK, byte 1 = command echo (confirmed on hardware), data from byte 8.
- Unsolicited: `A0` calibration, `A1 ?? idx` profile switched on keyboard, `A2` reset, `AA FA` lighting changed.

## Framing (group 0x55)
- Read: `55 cmd 00 sum len lo hi`, sum = (len+lo+hi)&FF, len ≤ 56.
- Write: `55 cmd 00 sum len lo hi last data…`, sum = (len+lo+hi+last+Σdata)&FF, last = 1 on final chunk.
- Simple reads: INFO `55 03 00 20 20` (fw = hex of LE u16 at data[0..2]), BASE `55 04 00 20 20` (data[0] = profile; > 3 → 0), PUBLIC_INFO `55 B2 00 09 09 00 00` (data[0..4] ASCII type, then year, month, serial[3]).

## Commands
| cmd | dir | region | notes |
|---|---|---|---|
| 05 / 06 | R/W | 64 B at 64·profile | settings block (below) |
| 07 | R | 512·layer + 2048·profile | factory keymap |
| 08 / 09 | R/W | 512·layer + 2048·profile | user keymap, 128 × 3 B |
| 0A / 0B | R/W | 512·profile | per-key RGB, 128 × 3 B |
| 0C / 0D | R/W | 2048·slot | macros |
| 0E | W | addr 0, 1 B | select profile |
| A0 / A1 | R/W | 1024·profile | key triggers, 128 × 8 B |
| A2 / A3 | R/W | 768·profile | DKS, 32 × 24 B |
| A4 / A5 | R/W | 256·profile | MT/RS/SOCD, 32 × 6 B |
| A6 / A7 | R/W | 128·profile | TGL, 32 × 3 B |
| never | — | — | EE reset, 5F boot, 80–84 firmware, A8/A9 calibration, group-6 `0F FF` factory settings |

## Key trigger (8 B)
- `[0]` 0xA0 | switch type (0 Qian 4.00 mm, 1 BeiDou 3.60 mm for type 101)
- `[1]` priority<<4 | key_mode (0 off, 1 RT, 2 continuous RT)
- `[2]`,`[3]&1` actuation−1 (0.01 mm); `[3]` bits 3–4 press precision, 1–2 release precision (0: 0.01, 1: 0.005, 2: 0.001 mm; UI only for types 107+)
- `[4]`,`[5]&1` RT press−1; `[5]>>1` top dead zone (0.01 mm, max 50)
- `[6]`,`[7]&1` RT release−1; `[7]>>1` bottom dead zone
- Limits type 101: actuation ≤ switch travel − bottom DZ (≤ 4.00), RT 0.01–4.00, DZ 0–0.50.
- Official single-key setter writes DZ−1 (inconsistent with its reader); Kraken uses the reader/bulk-writer form.

## Keymap entry `[type, code1, code2]`
- 16 key (code1 = modifier mask, code2 = HID), 32/33 mouse, 48 consumer (code1 | code2<<8), 64 system, 112 macro, 144 DKS, 145 TGL, 146 MT, 147 RS, 148 SOCD, 149 OKS, 240 function (`240 255 n` = FN n).
- Hardware (FB3E fw 121): 95 matrix slots; slots 95–127 hold filler `F0+layer` ×3 (written back as read). Empty / transparent = `00 00 00`. Factory FN2 layer contains `240 8 0` (keyboard reset) — never offered in Kraken's picker. FN1 has `240 250..253 0` "BOARD SW" (profile keys).
- Layout key → matrix slot: first unused slot of factory layer 0 (profile 0) with the same code (`K()`: plain HID, modifiers 224–231, FN1 255, FN2 254).

## Knob (type 101)
- Matrix slots 92 press (factory `48 205 0` Play/Pause), 93 right (`48 233 0` Vol+), 94 left (`48 234 0` Vol-); present in every layer. Official UI finds them by code (`Mute 205 / Vol- 234 / Vol+ 233`, type 103 uses 226 for press).

## Advanced keys (keymap entry -> table entry k)
- MT `146 k hold_ms/10` -> MT table [tap(3), hold(3)] at 256·profile + 6k.
- RS 147 / SOCD 148 / OKS 149 pairs: key A `t k slotB`, key B `t k+1 slotA`; MT entries k = [A, B], k+1 = [B, A]. Official rebuilds the whole MT table (`setAllMtKeyInfo`).
- TGL `145 k 0` -> TGL table 3 B at 128·profile + 3k. DKS `144 k 0` -> 24 B at 768·profile + 24k (points[4], 4 × (action 3 B + stage bits 2 B)).
- SOCD priority: trigger `[1]` high nibble (0 last input, 1 / 2); official also sets 1.5 mm actuation, 0.10 press/release on both keys (Kraken keeps travel).
- User board (fw 121) had two SOCD pairs at slots 88..91 (entries 0..3): factory slots 88 A, 89 S, 90 W, 91 D (matrix order is not row order; T = 19, G = 57).

## Settings block (64 B)
- `[1]` low nibble system mode (2 = mac) · `[4]` low = polling (1 8k, 2 4k, 3 2k, 4 1k), high = tick (0 off, 1 30, 2 60)
- `[6]` b0 lock Win, b1 lock Alt+Tab, b2 lock Alt+F4 · `[7]` b0 Adaptive Stable ("berserk"), b1 bottom-RT mode, b3 debug, b5–7 debounce
- Lighting: `[8]` effect, `[9]` brightness 0–100, `[10]` 4 − speed, `[11]` direction, `[12]` 0 = single colour, `[14..16]` RGB; logo `[24..31]`, side `[32..39]`.
- Polling-rate change re-enumerates the keyboard (official reconnects after 3 s; confirmed).
- Official lighting setter only ever writes `[12] = 0` (single colour) when a colour is chosen; never 1. Backlight off = brightness 0.
- Per-key colours (effect 0 "Custom"): cmd 0A read / 0B write, 128 × RGB at 512·profile (official writes 384 B).
- Public info type bytes read `FF FF FF FF` on fw 121 (unset).

## Web-driver feature inventory (bundle `gen2-bundle/app_page-feab963f2440235f.js`, js-beautify 1.15.1 line refs)
- Device API (L480-990, class `j`): getInfo 85[3], getBase 85[4], getFunc/setFunc 05/06 (64 B per profile), key matrix 07/08 R, 09 W, macros 0C R / 0D W (2048 B per slot), key colours 0A/0B, trigger A0/A1 (1024 B/profile, 8 B/key), DKS A2/A3 (768 B/profile, 24 B), MT A4/A5 (256 B... index*6), TGL A6/A7 (128 B), live LED colours DE (384 B = 128 x RGB, read-only, polled every 300 ms for the preview, L18957), profile select 0E.
- Never used by Kraken: calibration A8/A9 (input reports 0xA0 = calibration stream), reset EE, boot mode 95[06...], ROM erase/write 81/83, firmware update.
- Func-byte setters (L2346-2390): func[7] bit0 Berserk mode, bits1 stability (Adaptive Stable), bit3 debug mode key, bits5-7 debounce; func[4] low nibble polling, high nibble tick rate; func[1] system mode.
- Live depth: none on gen-2 (the Test tab uses browser keydown/keyup, L20835).
- Gaps vs Kraken (2026-11-11): DKS / Rappy Snappy / OKS editors, macros (record + bind), light direction, light position, sleep time, live LED preview (DE), import of the official config export.
