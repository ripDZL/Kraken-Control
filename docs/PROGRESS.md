# PROGRESS

## 2026-11-07
- Workspace created. dd-proto: model/identity (decimal fw), layouts (82 keys, official slots), factory keymaps (Master/Ultra, 3 layers), packets (identity, common B5, travel FD write/pull/parse, remap 42-pkt), picker catalog + labels.
- app: Win32 HID transport (type-checked via `check-win`), device worker (connect/reconnect, idle ping, read-back, verified saves), store, egui UI (Performance + Remap tabs, profiles, demo mode).
- Windows build OK. Hardware: remap save confirmed working, rest of layout intact (A75 Master fw 85).
- v0.2 (not yet hardware-tested):
  - `app/src/log.rs`: diagnostics log + rotation; packet hex toggle in UI; connect failures logged.
  - `dd-proto/src/rtp.rs`: LW/RDT clear/write/pull/parse, RT Match mode, slot count/select. 17 tests.
  - device: `Snapshot` (identity+travel+pairs+slots) on connect/Refresh/slot switch; ping detects keyboard-side slot change; SavePairs verifies LW by read-back; unknown slots in read-back dropped + logged.
  - UI: "Last Win / RDT" tab (click two keys to pair, RDT trigger/reset sliders, key badges), Key pairs Off/LW/RDT with official rules (exclusive, force RT on + Turbo off; RT off or Turbo on clears pairs mode), Full replace, RT Match + mode combo, Onboard slots buttons (+/-, blocked while unsaved), slot in top bar. Profiles store pairs/match/replace.
  - Demo: `--demo --tab=pairs|rdt|remap`.
- v0.2 confirmed on hardware by user (log: saves/reads/slot switches, no errors).
- v0.3 (not yet hardware-tested):
  - `--agent` tray mode (`agent.rs`): tray icon + menu (open UI, slots, per-app toggle, start with Windows, exit), foreground hook -> per-app slot via `agent.json` rules; opens HID only per switch.
  - Cross-process HID lock (named mutex) around all UI worker I/O + agent transactions; stale reports drained on lock.
  - Idle pings no longer packet-logged (log only identity changes).
  - `sys.rs`: single UI instance (focus existing), autostart registry, process names, running apps. `icon.rs`: code-drawn icon (window + tray).
  - UI tab "Per-app / tray": agent status/start/stop, start with Windows, rules (running-app picker or typed exe), default slot.
- Renamed to **Kraken Control** (package/bin `kraken-control`), user icon (`app/res/kraken.png` -> `.ico`), build.rs .res (icon + VERSIONINFO, byte-identical to llvm-rc output), tray icon from embedded .ico at system size, agent per-monitor DPI aware, legacy migration (old agent, Run value, %APPDATA% folder).
- Rename + icon confirmed on hardware (title, icon, settings moved from DeerControl).
- Actuation save writes only changed tables (AP/DS/US); DS/US slider min 0 (= keyboard default, official `Ue = 0`, keycap dims 0); keycaps hide default RT values.
- v0.4 (not yet hardware-tested): gen-2 support for the 2025 A75 Ultra (VID 19F5 PID FB3E).
  - RE of drunkdeer.keybord.net.cn bundle (user downloaded lazy chunks via PowerShell; site blocked from sandbox/VM) -> `docs/protocol-gen2.md`.
  - `dd-proto::gen2`: framing/checksums, region read/write, trigger record (raw-preserving setters), keymap actions + labels, layout K75 + official slot matching, settings fields. 6 golden tests.
  - `device2.rs` worker (own thread, shared HID lock, notifications A0/A1/A2/AA FA filtered, ping detects profile switch), `ui2.rs` panel: Performance (actuation, RT off/on/continuous, press/release, top/bottom dead zone), Keymap (read-only, 4 layers), Lighting (effect, brightness, speed, colour), Settings (polling, tick, debounce, Adaptive Stable, locks), profiles 1-4.
  - Board switcher in top bar when both keyboards are connected. Connect is read-only.
- v0.4 confirmed on hardware (gen-2 log 2026-11-08): replies echo the command byte; travel writes verified; settings write OK; polling 4k->8k re-enumerates and reconnects. Public type reads FF FF FF FF (unset). User's board was on effect 0 "Custom" -> v0.4 lighting showed only brightness ("no options").
- v0.5 (untested): gen-2 remap editor (4 layers, picker from official tables minus Boot/Reset/wireless/side-light), per-key colour painting for "Custom" (brush, right-click pick, fill), effect button grid, backlight toggle, colour picker sets single-colour (no rainbow write), profile names (Default/Game/Office/Custom), factory layers read for Restore/Reset layer.
- v0.5 confirmed on hardware (remap FN1 verified, colours verified, many settings saves).
- v0.6 (untested): knob remap (slots 92/93/94 found via factory Play/Vol-/Vol+), labels shrink/wrap to fit keycaps, "Advanced keys" tab: list Main-layer SOCD/RS/OKS pairs, Mod Tap, Toggle, DKS; create SOCD (priority), Mod Tap (tap/hold/hold time), Toggle; remove any (restores factory action, clears entries). Tables MT (A4/A5) and TGL (A6/A7) written before keymaps, verified by read-back.
- v0.6 confirmed on hardware (2026-11-08): knob P/L/R remap, SOCD add/remove + priority (S=1/W=2 on W/S pair), Mod Tap add/remove. Toggle untested.
- v0.7 (uncompiled, untested): gen-2 Lighting live apply (effect fields + per-key colours written 250 ms after last edit, never mid-stroke, one write at a time; no Save needed), drag painting with segment sampling, Undo stack (30 strokes), `merge_pending` keeps unsent edits when Func/Colors replies land.
- v0.7 confirmed running: drag painting + live per-key colour writes; backlight toggle live. v0.7.1 (uncompiled): `light_due` replaces 250 ms debounce (painting/clicks immediate, held slider waits), Fill/Undo reordered + Fill click logged.
- v0.7.1 confirmed (user). v0.8 (uncompiled UI): `crates/dd-proto/src/pattern.rs` (pure: Direction, Group, positions/gradient/rainbow/random/hsv, 6 tests) + Lighting Highlight/Pattern rows; Fill now goes through `set_key_colors`.
- v0.8 confirmed (user). v0.8.1 (uncompiled UI): pattern.rs + PRESETS/palette/stripes/checker (row rank by centre height, parity via `& 1`), UI Stripes/Checker/palette combo.
- v0.8.2 (uncompiled): portable mode (`portable` marker next to exe -> `data\` folder, no %APPDATA%/registry migration), `packaging/` (package.ps1/.cmd static-CRT zip, README-TESTERS.txt).
- v0.9 (uncompiled UI; `dd-proto::led` 4/4 tests + clippy standalone): gen-1 Lighting tab: firmware effects (AE 01 effect packet), brightness/speed 0-9, preset colours + colourful, Custom (per key) with drag paint/fill/undo/patterns (7 AE packets, mode 19), live send, state persisted per model in state.json `lighting`.

## 2026-11-08 (UI overhaul, v0.10.0 — uncompiled)
- Design direction agreed via mockups (claude.ai Design canvas "Kraken Control UI": Abyssal, Black-Plumb pages). User picked Black-Plumb + asked for Dark Grey (default).
- `app/src/theme.rs` (new): `ThemeId` DarkGrey (default) / BlackPlumb, `Palette` colour roles, fonts (Atkinson Hyperlegible Next for Dark Grey, JetBrains Mono for Black-Plumb; Latin subsets in `app/res/fonts`, OFL texts beside them), egui Style/Visuals, `Page` enum, widgets (card, page_header, setting, switch, primary, pill, nav, status_line, keycap, plate, no_keyboard). Compiled + screenshotted standalone against eframe 0.31.1 in cloud scratch crate (both themes).
- Shell (both boards): header (icon, name, board selector, connection pill + firmware tooltip, profile, unsaved count, Reload, Save), left page menu (Performance, Keymap, Advanced keys, Lighting, Profiles, Settings; amber dot = unsaved on that page), status bar, "Plug in your keyboard" empty state.
- Gen-1: side panel removed — RT/Turbo/RT Match -> Performance "Whole keyboard" card (switches); Selected keys card with helper text; key pairs mode -> Advanced keys page; onboard slots + PC profiles + per-app rules -> Profiles; tray/autostart + troubleshooting + theme -> Settings. Wording: slots -> profiles, "Log packets (hex)" -> "Detailed log for bug reports".
- Gen-2: `central()` -> `body()`, side panel -> Profiles page, keyboard settings card + shared Appearance/Troubleshooting on Settings. Editors themselves unchanged (inside cards).
- Keycaps: two-tone (skirt + top face) for both boards; all hard-coded UI colours now palette roles (data colours untouched).
- `store::Settings.theme` (serde default = Dark Grey). Demo args: `--tab=` / `--g2tab=` accept page names (old names kept), `--theme=plumb` (not saved).
- package.ps1 copies font licences into the zip (`licenses\`). Version 0.10.0.
- v0.10.0 built + ran on Windows (user screenshot, gen-2 A75 Ultra connected). Feedback: keyboard tiny (cause: keyboard drawn inside `ui.horizontal`, whose available height is only interact_size.y, so the height cap clamped unit to 30).
- v0.10.1 (uncompiled app; theme.rs change compiled + screenshotted in cloud harness): keyboard drawn in `vertical_centered`, height cap only on pages whose editor scrolls its own list (`fit_height`: gen-1 Keymap/Advanced, gen-2 Keymap). Header swapped: Reload / Save / unsaved count on the left; profile + keyboard pill on the right. Board selector buttons removed: with both keyboards connected the pill shows a swap icon and switches board on click. Narrow window (< 1100 px header) moves the profile into the pill tooltip. App icon + name moved to the bottom of the page menu above the version.

## 2026-11-10 (v0.10.2 — compiled + screenshotted in cloud, not yet built on Windows)
- v0.10.1 built + ran on Windows (user screenshot). Feedback: keyboard size differed between pages; wants lighting presets.
- Keyboard size: `theme::key_unit(ui)` = available width / 16.5 (30..64), used by both boards; `fit_height` removed. Every page now sits in the page ScrollArea; inner ScrollAreas (gen-1 Keymap list, gen-1 pairs list, gen-2 Keymap list) replaced by `ui.scope`, so the keyboard is the same size on every page and board (verified by screenshots).
- Lighting presets (both boards): `theme::presets_row` (chips: click = apply live, right-click = Replace with current lighting / Delete; "Save current…" menu with auto-focused name field, Enter saves, same name replaces). Active preset highlighted when it matches the current lighting. Stored in state.json: `light_presets` (gen-1 `LightPreset1 {name, model, light: Lighting1}`) and `light_presets2` (gen-2 `LightPreset2 {name, board, fields (ui2::LIGHTING order), colors}`). Gen-1 apply -> `light_live` sends + saves; gen-2 apply sets func fields + colours -> `live_lighting` (SaveFunc + SaveColors). Applying pushes a colour undo step. `Gen2Panel::body(ui, presets) -> bool` (host persists).
- Verified in cloud: cargo build/clippy (incl. `--features check-win`) clean, tests 37+4 pass, Linux release 4.8 MB; demo screenshots of all keyboard pages both boards; synthetic clicks (python-xlib XTest under Xvfb) for save (Enter), apply (gen-1 effect + per-key, gen-2) and right-click menu.

## 2026-11-10 (v0.10.3 — memory pass, compiled + measured in cloud)
- User numbers (v0.10.2, Windows): Task Manager 54 MB (private working set), Process Explorer private bytes 148 MB.
- New `app/src/mem.rs`: counting global allocator (Kraken heap now/peak) + OS counters (Windows `GetProcessMemoryInfo` PrivateUsage/WorkingSetSize; Linux /proc RssAnon/VmRSS). Logged 10 s after start and every 30 min (`memory: ...`), shown live in Settings > Troubleshooting. windows-sys feature `Win32_System_ProcessStatus`.
- Finding (Linux demo, llvmpipe): Kraken heap 2.9 MB (peak 7.7 MB), font atlas 16384x32. So >130 MB of the 148 MB private bytes on Windows is outside Kraken's heap (OpenGL driver + GL buffers).
- Trims: eframe `default_fonts` off (egui's 4 fonts, ~1.4 MB) -> own fallbacks `KrakenArrows.ttf` (Hack subset ← ↑ → ↓, MIT/Bitstream Vera licence) + `KrakenStar.ttf` (emoji-icon-font subset ★, MIT); `raw_input_hook` caps max_texture_side at 2048 (font atlas width); keycap fonts in whole pixels (`theme::key_unit` floors, `theme::key_font` rounds) so resizing doesn't add font sizes. package.ps1 copies all font licence .txt files.
- Result (Linux): heap 1.4 MB (peak 3.2 MB), atlas 2048x64, process anonymous memory 45.3 -> 42.4 MB, release exe 4.83 -> 3.48 MB. Glyphs checked (★ renders; cmap covers ← ↑ → ↓). CJK key names (変換) were already missing before.
- Big lever left: the renderer. Software rendering avoids loading the GL driver; `egui_software_backend` needs egui >= 0.33 (we pin 0.31.1). Options: upgrade egui + that crate, or own small CPU rasteriser + Win32 GDI present. Not started — user decision.

## 2026-11-10 (v0.11.0 — software renderer, alignment, contrast; compiled + tested in cloud, Linux only)
- Software renderer (default): `app/src/soft.rs` (winit 0.30.13 + egui-winit 0.31.1 event loop, repaint scheduling via request_repaint_callback + EventLoopProxy, softbuffer =0.4.6 present) + `app/src/raster.rs` (CPU rasteriser: premultiplied gamma-space like egui_glow, top-left fill rule, per-row span from edge functions, opaque span fill, text fast path, bilinear textures, row bands per thread). No OpenGL driver loaded.
  - Linux measurements (release, demo): software private 4.9 MB / working set 19.8 MB vs GPU (Mesa llvmpipe) 41.4 / 117.6 MB. Frame 7.5 ms raster at 1180x760 on 2 vCPUs. Screenshot diff vs GPU: 83 of 897k pixels differ > 24.
  - Click/typing/menus verified under Xvfb (python-xlib XTest). Debug-build sweep of all pages, both boards, both themes: 0 panics (an overflow in iceil was found and fixed; regression test `extreme_triangles_are_safe`).
  - NOT verified: Windows build/run (static.rust-lang.org blocked, no windows target std in cloud).
  - Settings > Drawing: Processor (low memory, default) / Graphics card; stored `settings.renderer` (`Renderer::Software|Gpu`); "Restart now" spawns `--restarted` (waits up to 5 s for the UI mutex). CLI `--gpu` / `--software`. If software can't open a window, process restarts itself with `--gpu --restarted` (winit allows one event loop per process).
  - `ui::App::new(&egui::Context)`, `App::frame(ctx)`, `App::input_hook(raw)` shared by both renderers; eframe impl delegates. `theme::apply` forces dark theme preference + `set_style_of(Dark)` (a light Windows theme no longer switches egui to its light style).
- Keyboard alignment: `theme::page_header_with` (fixed 34 px title row with optional controls + one truncated blurb line); Lighting presets moved into the title row on both boards. Keyboard card starts at the same y on every page (screenshots).
- Contrast: test `theme::tests::all_text_is_readable` (WCAG 4.5:1 text, 3:1 disabled/graphics incl. egui's 50/50 disabled tint) for both themes; `key_ink_is_readable_on_any_colour` (ink_on now picks black/white by WCAG contrast). Fixes: `ui.strong()` used active fg (dark on-accent) -> active.fg_stroke = text; hint text via `theme::hint`; disabled primary button drawn plain (`theme::primary_button`); egui `.weak()` category labels -> muted; palette tweaks (DG muted ABAFB6, disabled 7D8188, key_selected 1F3B3A, key_hover 3F4248; BP input 44555B, key_top 334146, key_hover 384750, key_selected 24342A, error FF8A75, key_tag 8BD452); travel colours `theme::travel_color` (blue 8CC3FF -> orange).
- Performance keycaps: press/release pair drawn only where it fits between name and value (`theme::key_text_if_fits`), full values in a hover tooltip on every key (both boards); values shown as "0.5" style (`theme::mm`).

## 2026-11-10 (v0.12.0 — profile export / import; compiled + tested in cloud, Linux only)
- `app/src/backup.rs`: JSON file `{format: "kraken-control-profile", version: 1, app_version, created, keyboard?, kraken?}`.
  - `keyboard`: tagged `family` gen1 (`model`, `settings: Profile`, `lighting: Lighting1`) or gen2 (`board`, per-slot `keys` {actuation, mode, press, release, top_dz, bottom_dz, priority}, `keymap` layers of [kind, code1, code2], `settings` by name (`ui2::BACKUP_FIELDS`), `colors`, `mt`, `tgl`).
  - `kraken`: theme, renderer, light presets (both), PC-saved profiles, agent config.
  - Checks on import: travel clamped to page ranges (NaN -> default); key pairs must be valid distinct layout slots within limits, pair rules as the UI (exclusive, need RT on + Turbo off); gen-1 key actions must be NONE / factory / catalog, else factory; lighting ranges; Kraken part: invalid saved profiles dropped, gen-2 presets checked, app rules normalised, slots capped. Gen-2 (`Gen2Panel::import_backup`): board must match, sizes must match; triggers via setters (switch type / precision kept) with ACTUATION_MM / per-key max / RT_MM / DEADZONE_MM clamps; actions must be picker / factory / NONE; MT/SOCD/RS/OKS/TGL keys only if their table entries hold such actions (bounds checked); DKS keys left as on the keyboard; settings by name, value must be one the page offers or the current one. Notes reported in status + log. Newer format refused; files > 8 MB refused.
  - Applies as unsaved edits (review, then Save to keyboard); gen-1 lighting sent immediately (no Save step there). Kraken part merged by name (presets / profiles / app rules), theme applied live.
- UI: Profiles page card "Move to another PC" (both boards): Export to file…, Import from file… -> preview with checkboxes (keyboard part disabled with a note when it is for another keyboard), Import / Cancel. Export includes unsaved edits (stated). File dialogs: `sys::file_dialog` = Win32 GetSaveFileNameW / GetOpenFileNameW on a worker thread (COM STA, owner = Kraken window); non-Windows uses `<data dir>/kraken-profile.json` (demo tests). windows-sys features `Win32_System_Com`, `Win32_UI_Controls_Dialogs`.
- Status bar text truncates with hover for the full message.
- Verified (Linux demo, XTest clicks): gen-1 export -> tampered file (ap 99, unknown action, bad pair, theme, preset, odd app rule) -> import: all checks fired, values applied; gen-2 export -> tamper (polling 99, macro action, DKS, actuation 99, colour) -> import -> re-export confirms. Tests: backup round trip / format / newer version / gen-1 checks. NOT verified on Windows (file dialogs untested).

## 2026-11-10 (published)
- Public repo github.com/ripDZL/Kraken-Control (created as Karken-Control, renamed), branch `main`, root commit 0a2f45b. Author = GitHub noreply address only (old repo's web-UI initial commit exposed the account email; repo deleted + recreated).
- Pre-publish audit: no emails/tokens/keys/local paths; AI_CONTEXT path scrubbed; screenshots metadata-stripped; protocol docs prose only; `crates/dd-proto/data/` = extracted factual tables. Added README (features, safety, build, credits incl. svumo/DrunkDeer-Control MIT), LICENSE (MIT), `.gitignore` (/KrakenControl/, /data/, exported profiles), `docs/screenshots/`.
- Release pipeline: `.github/workflows/release.yml` + `.github/release-notes.md`; zip now ships LICENSE; README Install points to Releases; README-TESTERS reworded for public use (issues link). Tag v0.12.0 pushed -> first draft release.
- Release v0.12.0: Release run passed on windows-latest (tests green on Windows); draft has zip (exe 3.1 MB) + .sha256, checksum verified. Draft body "What's new" = CHANGELOG 0.12.0 (paste manually: session can't edit releases or repo settings).
- Repo polish: README header (badges, links, gallery: keymap, advanced-gen2, lighting-gen2-plumb), CI workflow `ci.yml` (Windows clippy -D warnings + tests), issue forms (bug/feature), PR template, CONTRIBUTING.md, SECURITY.md, CHANGELOG.md, `docs/social-preview.png` (upload in Settings > Social preview).

## 2026-11-11 (website)
- `site/`: one-page site (plum + saffron from icon / Black-Plumb; Atkinson Hyperlegible Next + JetBrains Mono, self-hosted woff2 + OFL). Hero: draggable keycap + travel gauge simulating actuation point and Rapid Trigger (press/release markers, press counter; mouse, touch, arrows/Space). Sections: screenshot, features (zoomed crops), keyboards, safety, install, FAQ. Download links use the latest release asset via api.github.com.
- `.github/workflows/pages.yml` deploys on site/screenshot/icon changes. Pages enabled (source GitHub Actions) via built-in browser.
- Checked: Playwright at 320/360/390/768/1024/1280/1440 (no horizontal scroll, no console errors), drag and keyboard control of the demo.
- Website v2 (user: ditch purple, more professional, grey promo shots, real keyboard in demo): light paper page with the app's Dark Grey for demo/screenshot band/footer, teal accent, no sticker outlines. Demo tabs: Simulate + Your keyboard (live press/release timeline, presses, per second, quickest re-press, shortest press; touch devices get a note). Depth from a real keyboard is not readable by a web page (would need WebHID + unverified protocol); live depth stays a possible app feature (TODO P2 key-depth view). Lighting promo shot recaptured in Dark Grey (`docs/screenshots/lighting-gen2.png`), plumb shot removed. Checked 320–1440 px, no errors.

## 2026-11-11 (web-driver parity, part 1)
- Inventory: gen-2 bundle staged from `gen2-bundle/` (read-only folder access); gen-1 bundle re-read in the built-in browser (cloud and device shells can't reach beta.drunkdeer-antler.com; old capture folder empty). Facts in protocol docs.
- Gen-1 live key depth: `dd-proto::packets::DepthFrame` (FD 06, golden tests), device worker polls `FD 03 01` 20 ms after each frame while on (quiet logging, stops after 5 empty frames, sends `FD 03 00` when off; `request()` ignores stray FD 06). UI: Performance > Whole keyboard > Show live key depth (disabled while the keyboard has Turbo on), shading + amber actuation line on keys, tooltip shows depth. Demo: `--demo --tab=performance --live-depth` animates W/A/S/D.
- Gen-1 knob: `dd-proto::knob` (6 events, golden tests), `Cmd::SaveKnob` (C1 acks), store `knobs` per model, Keymap page: click knob -> knob editor (picker), Save writes all six; backup `Gen1.knob` (optional, checked against the picker).
- Tests: dd-proto 44, app 16; clippy clean both configs. Hardware untested.

## 2026-11-11 (web-driver parity, part 2: gen-2 DKS / RS / OKS)
- Repo state at start: `main` a562e15, CI green; local folder on the PC was 5 commits behind (user runs `git pull`).
- Bundle: `gen2-bundle/*.js` staged read-only, beautified with js-beautify 1.15.1 (`app_page-feab963f2440235f.js`, SHA-256 `7cdf9668…a9de82`); DKS / RS / OKS facts + line refs in `docs/protocol-gen2.md`.
- `dd-proto::gen2`: `Dks` / `DksBind` (spans per stage), `dks_stage_bits` / `dks_spans` (official writer / reader incl. continuation bits), `dks_entry` / `set_dks_entry` / `clear_dks_entry`, `dks_point_range`, `dks_span_limit` / `dks_normalize`, `dks_entries_used`, `cmd::DKS_WRITE` (A3). Golden tests `dks_stage_bits_match_official`, `dks_entry_golden`.
- App: `device2::Table::Dks` (write + read-back verify like MT / TGL), `Gen2Panel.dks` working copy, `Draft::Pair { kind }` (SOCD / RS / OKS share one path; priority only for SOCD), `Draft::Dks` (new or Edit of an existing entry), `dks_editor` (four depths, action x stage grid: off / Tap / Hold to <stage>; longer hold clears the stages it passes), `dks_summary` in the list. Remove clears the DKS entry.
- Back-up: `Gen2.dks` (serde default; older files keep the keyboard's DKS keys). Import accepts DKS keys whose four actions are picker / factory actions.
- Demo: Q = DKS; `--g2draft=dks|rs|oks` opens an editor for screenshots. `docs/screenshots/advanced-gen2.png` retaken.
- Verified (Linux demo, XTest clicks): Edit -> Press "Hold to Top" -> Apply -> list shows "Q (Press to Top)", page marked unsaved. Tests: dd-proto 46, app 16; clippy clean; check-win OK. Hardware untested.
- Kept from SOCD: RS does not change the two keys' travel (official writes actuation 1.50, press / release 0.10 mm); entries are freed in place, not renumbered.

## 2026-11-11 (web-driver parity, part 3: gen-2 macros)
- Bundle facts completed (repeat count is always 1, recorder = key events only, 16 macros, 496 steps per profile, reader quirk at offset 64) -> `docs/protocol-gen2.md` "Macros".
- `dd-proto::gen2`: `MacroEvent` / `MacroKey`, `encode_macros` (official byte layout), `parse_macros(raw, bound)`, `macro_action`, `macros_bound`, `macro_base`, `cmd::MACRO_READ/WRITE`. Golden test `macro_block_golden`.
- App: snapshot reads the macro block (read failure -> macros off, rest works); `Cmd::SaveMacros` + `Event::Macros`; Advanced page second card "Macros": list (steps, keys it is on, Edit / Put on a key / Take off keys / Delete), New macro (lowest free id of 16), editor (Record / Stop, Keep my timing, Add key, per-step Press / Release + key + pause, unpaired press warning blocks Apply). Recorder reads egui key events + modifier state (Ctrl / Shift / Alt as left keys; Win not seen).
- Back-up: `Gen2.macros` (serde default); import takes macro keys only when the file's macro is not empty.
- Demo: M1 = Ctrl+C; `--g2draft=macro` opens the editor. Verified on Linux (XTest): Clear, Record, typed Shift+H... -> steps listed in order. Needs keyboard focus on the window (set by hand under Xvfb).
- Tests: dd-proto 47, app 16; clippy clean both configs. Hardware untested.

## 2026-11-11 (web-driver parity, part 4: gen-2 lighting extras)
- User ran a655f15 on the 2025 A75 Ultra: "everything seems okay" (DKS / RS / OKS / macros; not itemised). Asked for live key actuation on that board: not in the official protocol (documented in TODO).
- Bundle: direction values per effect, light point (type 102 only), live colour read DE, no sleep time -> `docs/protocol-gen2.md` settings block.
- `gen2::effect_directions`, `cmd::LED_READ`, `LED_LEN`; test `lighting_direction_and_live_read`.
- App: Direction row on Lighting for effects 6 / 7 / 8 / 10 (`LIGHT_DIRECTION` now in `EDITABLE`, `LIGHTING` (last; presets with one field less still apply) and `BACKUP_FIELDS`). Live view card below the controls (`Cmd::ReadLeds` / `Event::Leds`, 300 ms poll, quiet).
- Verified: Linux demo screenshot (Direction row, Live view with stand-in colours). Tests: dd-proto 48, app 16; clippy clean both configs. Hardware untested.

## 2026-11-11 (web-driver parity, part 5: official config import)
- Bundle: export / import handlers + `setAllAdvancedKeys` -> `docs/protocol-gen2.md` "Official config export".
- `backup::parse_official` (pure): travel -> `Gen2Key`, four layers, tables + advanced keymap entries rebuilt from `advancedKeys` in list order, DKS continuation values folded into spans; `OFFICIAL_BOARD` marker skips only the board-name check in `import_backup`; empty colours = keep. Test `official_export_is_converted` (synthetic file built from the field names in the bundle).
- UI: help line on the back-up card (gen-2). Tests: dd-proto 48, app 17; clippy clean both configs. No real export file seen yet.

## 2026-11-12 (live view speed)
- User (2025 A75 Ultra): Live view works, looks slow. Cause: 300 ms poll (official interval) = ~3 pictures a second, shown as jumps.
- Fix: `LED_POLL` 60 ms, fade from the on-screen colours to each new read over the measured read interval, repaint ~30 fps; worker logs the first read's duration once per run. Hardware untested.

## 2026-11-12 (official import: dialog fix)
- User tried a real official export (`TESTA asd.json`): file parsed, but the import dialog greyed out the keyboard part with "These are for the official driver export; the keyboard shown now is the A75 Ultra (2025)". Cause: `backup_card` compares `Keyboard::name()` with the shown keyboard; the `OFFICIAL_BOARD` exception was only in `import_backup`. Missed because the dialog path was not exercised (no way past the native file dialog under Xvfb).
- Fix: `Keyboard::official()`; dialog offers official files on any connected gen-2 board, header "Exported by the official web driver", own checkbox / help text. New `--import=<file>` argument opens the dialog directly; used to verify dialog -> Import -> "Unsaved changes" in demo.
- Still wanted: the user's JSON as a fixture (format so far confirmed only as far as "it parses").

