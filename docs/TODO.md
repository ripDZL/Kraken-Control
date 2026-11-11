# TODO

## P0 — hardware checks (user)
- [x] `cargo build --release` on Windows (needed `%USERPROFILE%\.cargo\bin` on PATH).
- [x] Remap save works on hardware without scrambling layout (user, 2026-11-07).
- [x] v0.2 LW/RDT/RT Match/Full replace/slots: user "everything seemed to be good" (log 2026-11-07, no errors; RDT bytes not packet-logged).
- [ ] Save one AP change -> "verified by read-back" (not explicitly confirmed).
- [ ] After slot switch, do remaps differ per slot? (decides per-slot keymap store)
- [x] v0.3 rename: title, icon, settings migrated (user, 2026-11-07). Task Manager name/version not yet confirmed.
- [ ] v0.3 tray: agent starts from UI, tray menu works, slot switch by app (rule + default), autostart survives reboot, UI open while agent runs (no "keyboard busy" errors).

- [x] v0.4 gen-2: connect, read, labels, key travel saves verified, settings save, polling-rate change + reconnect (user log 2026-11-08).
- [x] v0.5 gen-2: remap (Main + FN1) verified, lighting + Custom per-key colours verified (user, 2026-11-08).
- [x] v0.6 gen-2: knob press remap (slot 92 Play/Pause -> consumer 0x192 -> back), both verified by read-back; existing SOCD pairs (A/D, W/S) listed OK (user log 2026-11-08 08:17Z).
- [x] v0.6 gen-2 (log 09:41–09:46Z): knob left/right remap + revert; SOCD T/G add (MT entries 4/5, keymap 148 k slotB) + remove, existing A/D, W/S kept; W/S priority write S=1, W=2 (A1). All verified by read-back.
- [x] v0.6 gen-2: SOCD priority + Mod Tap (slot 12: tap I / hold Z, 200 ms; 146 4 20) add/remove work on keyboard (user, log 09:55Z).
- [ ] v0.6 gen-2: Toggle add/remove (no 145 / A7 write yet).
- [x] v0.7 built + ran on Windows (2026-11-08); drag painting + live colour writes work (log 10:07–10:09Z).
- [x] v0.7.1 (user OK 2026-11-08): painting writes mid-stroke (one in flight, next follows), clicks immediate, held slider/picker after 150 ms pause or release. Fill/Undo moved before Brush picker (picker popup may have covered them).
- [x] Fill bug (not reproduced in v0.7.1, user OK): v0.7 log showed Fill P (82 keys) written + verified twice, but a later Fill attempt produced no write (10:08:18–10:08:28; user then pressed Read and painted by hand). Cause unknown; v0.7.1 logs `gen2 ui: fill all keys` per click — check next log.

- [x] v0.8.2 portable package: built on Windows, tester ran the zip on another PC (2026-11-08); release zips now built by CI. Not yet confirmed: writes only to `data\`.

- [x] UI overhaul phase 1 (v0.10–v0.11). Remaining items: see "UI overhaul" below.

## P1
- [x] Diagnostics log (%APPDATA%\KrakenControl\log.txt, packet hex toggle).
- [x] LW / RDT pairs (FC 0A clear, FC 01 / FC 03 write, FC 05 / FC 07 pull).
- [x] RT Match + auto modes (B5 byte 11, FD 0C) and LW full replace (FC 0B).
- [x] Onboard slots fw>=81 (A2 count, FB 05 switch, identity byte 20). AB delete intentionally not used.
- [x] Tray agent (`--agent`), start with Windows (HKCU Run), per-app slot switching, single UI instance, generated icon (v0.3, untested on hardware).

- [x] Gen-2 remap (cmd 09, 4 layers, safe picker), per-key RGB (cmd 0A/0B), lighting effect grid (v0.5, untested).
- [x] Gen-2 knob remap + advanced keys SOCD/MT/TGL create/remove, RS/OKS/DKS list/remove (v0.6, untested).
- [x] Gen-2 DKS / RS / OKS editors (2026-11-11, cloud-tested in demo; hardware untested).
- [ ] Gen-2 light direction, advanced keys on FN layers.
- [x] v0.8 (user OK 2026-11-08; `dd-proto::pattern` tested standalone 6/6 + clippy): Custom lighting Highlight groups (WASD/Arrows/F-keys/Numbers/Modifiers by factory HID code) + Pattern (direction; Gradient brush->2nd colour, Rainbow 5/6 wheel, Random) -> `set_key_colors` (one undo step, live write).
- [ ] v0.8.1 (compiled since v0.10; hardware check pending): Stripes, Checker (brush + 2nd colour), palettes Sunset/Ocean/Forest/Fire/Vapor/Ice (3-stop blend along direction).
- [ ] Firmware effects using per-key colours (user request): official UI only writes func [12]=0 (single); unknown whether any [12] value makes effects use the 0B table. Needs bundle check (no guessing per CLAUDE.md).
- [ ] Gen-2 animated custom lighting: Animated host effects only if bundle has a RAM-only live RGB command (search `gen2-bundle` for music/screen-sync); repeated 0B writes may wear storage (unknown if 0B is flash). Bundle folder access was blocked this session.
- [ ] Tray agent per-app switching for gen-2 profiles (cmd 0E).
- [ ] Other gen-2 boards (FB5C A75 Pro, FB5E A75 Master gen-2 / layout K75M, X60) — need layouts from bundle.

## P2
- [x] Knob remap (C1 03, 6 events) and live key depth (FD 03 + FD 06) — v0.13, cloud-tested (demo), UNTESTED on hardware.
- [ ] Lighting (AE 01, custom per-key). Tester (friend, A75 Master) asked for Lighting 2026-11-08. -> v0.9 gen-1 Lighting tab written (effects + custom per-key + patterns), UNTESTED on hardware; knob/key depth still open. Needs gen-1 bundle `DrunkDeer-Control\tools\captures\antler-beta\index.exKUY8vl.js` (folder request denied by auto mode; user must add folder).
- [x] Performance pass 1 (v0.10.3): memory counters, default fonts dropped, atlas width 2048, whole-pixel key fonts. GL config already minimal (no MSAA/depth/stencil); agent already windowless.
- [x] Performance pass 2 (v0.11.0): software renderer (default) — Linux-verified only.
- [ ] Windows (builds since v0.11; user: "looks good so far"): check software renderer (window, typing, clipboard, DPI scaling, resize, minimise/restore), memory vs GPU, frame ms maximised.
- [ ] Other models: extract tables from bundle (tools in C# fork `tools/antler-extract/`).
- [ ] Drag-rectangle multi-select.

## Web-driver parity (user request 2026-11-11)
- [x] Gen-1: live key depth (Performance > Show live key depth; needs Turbo off).
- [x] Gen-1: knob, 6 actions (Keymap > click the knob).
- [ ] Hardware: both of the above on A75 Master / Ultra (check log for FD 06 frames, C1 acks).
- [x] Gen-2: DKS / Rappy Snappy / OKS editors (Advanced keys > Add; DKS rows have Edit).
- [ ] Hardware (A75 Ultra 2025): DKS create / edit / remove -> log shows `Saved Dks table (verified by read-back)` and the key behaves as set; DKS key made in the official driver reads back the same spans; RS pair (does it work with travel left as is? official writes 1.50 / 0.10 / 0.10 mm to both keys); OKS pair.
- [x] Gen-2: macros (Advanced keys > Macros: record / edit / put on a key; 2026-11-11, demo-tested).
- [ ] Hardware (A75 Ultra 2025): log line at connect (no `macro block not readable`); record a macro, Save -> `Saved macros (verified by read-back)`, key plays it; Reload keeps it on the same M number; macro made in the official driver shows up; Win / right-hand modifiers picked by hand play correctly.
- [ ] Macros: names, mouse steps, repeat count (official never writes anything but 1), macros on FN layers (bind is Main layer only).
- [ ] Gen-2: light direction, light position, sleep time, live LED preview (DE).
- [ ] Gen-2: import the official driver's config export.

## Release / repo
- [x] v0.12.0 draft release built by CI (zip + .sha256).
- [x] Published v0.12.0 (2026-11-10, Latest) with CHANGELOG notes; topics set; private vulnerability reporting on.
- [x] Social preview set to `docs/social-preview.png` (verified identical SHA-256).
- [ ] Optional: release workflow fills What's new from CHANGELOG.

## Back-up (v0.12.0)
- [ ] Windows: Export / Import dialogs, import into a fresh install, Save to keyboard after import (both boards).

## UI overhaul (v0.10.0)
- [x] Builds on Windows (v0.10 onwards; CI on every push).
- [ ] Check both themes on every page (Settings > Appearance), window at 1180x760 and minimum 960x600.
- [x] Same keyboard size on every page, both boards (v0.10.2).
- [x] Lighting presets, both boards (v0.10.2, cloud-tested; hardware untested).
- [ ] Gen-2 Performance: same Selected keys / Whole keyboard cards as gen-1.
- [ ] Restyle Keymap / Advanced / Lighting editors (setting rows, helper text).
- [ ] Optional: load the user's theme JSON directly instead of the built-in Black-Plumb copy.
