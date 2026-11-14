# AI_CONTEXT — drunkdeer-rs

## Identity
- **Kraken Control** (renamed from "Deer Control" 2026-11-07; repo folder still `drunkdeer-rs`). Native Rust rewrite of DrunkDeer control app. Replaces C# fork `../DrunkDeer-Control` (WPF, ~200 MB) — C# kept as reference only.
- Repo: github.com/ripDZL/Kraken-Control (public, branch `main`). Local folder `drunkdeer-rs`. Credit svumo/DrunkDeer-Control (MIT) in README.
- User board: A75 Master (identity `0b 05 04`, fw 85 decimal), user called it "A75 Ultra gen1". Win10 LTSC. Brave for official-site captures.

## Stack / decisions
- Rendering: software renderer (soft.rs + raster.rs, winit + egui-winit + softbuffer =0.4.6) is the default; eframe/glow kept as "Graphics card" option and fallback. Keep winit =0.30.13 / egui-winit =0.31.1 in step with eframe =0.31.1.
- egui/eframe `=0.31.1`, `glow` renderer, default-features off. NO Tauri, no WebView2.
- HID: raw Win32 via `windows-sys 0.59` (SetupAPI + overlapped ReadFile/WriteFile). No hidapi/C deps.
- Workspace: `crates/dd-proto` (pure protocol, tests), `app` (package/bin `kraken-control`: UI by default, `--agent` = tray mode without window/GL).
- Release profile: opt-level s, lto, codegen-units 1, panic abort, strip -> ~4 MB Linux, 3.1 MB Windows static-CRT exe (v0.12.0 release).
- State: `store::data_dir()` = `<exe dir>\data` if a file `portable` sits next to the exe (portable copy; legacy migration skipped), else `%APPDATA%\KrakenControl\` (migrated from `DeerControl` on start) — `state.json` (keymaps, profiles, settings), `agent.json` (per-app rules), `log.txt`.
- Autostart: HKCU Run value `KrakenControl` = `"<exe>" --agent` (legacy `DeerControl` value migrated).
- Named objects: mutexes `Local\KrakenControlHid|Ui|Agent`, agent window class `KrakenControlAgent`, UI title "Kraken Control".
- Icon: user-supplied `app/res/kraken.png` -> `kraken.ico`; vector source `app/res/kraken.svg` (user's art, cropped to the tile; shown at top of README); exe icon + VERSIONINFO via `app/build.rs` (.res written directly, MSVC only).

## Protocol essentials (see docs/protocol-a75-master-ultra.md)
- HID: VID 0x352D, usage page 0xFF00, output report ID 4, 63-byte payload, one request in flight, reply first byte == request first byte.
- Model by type bytes only. fw = decimal concat bytes[8],[7].
- Actuation: `FD 01/04/05` x5 pkts, u16 LE mm*200; pull `FD 07 01/03/04` -> 5x `FD 08/0A/0B`.
- Remap: 42 pkts `A0 02 04 pkt 0E group`, 6-byte entries by keyType; full layers from official factory tables + edits.
- Keymap NOT readable from keyboard -> app copy is source of truth; only written when edited.

## Constraints
- Never flash firmware. Never copy official bundle code (facts/data only).
- Only A75 Master/Ultra supported; other models must stay read-only/unsupported until their tables are extracted.
- Website: `site/` (static HTML/CSS/JS, self-hosted OFL fonts) -> `.github/workflows/pages.yml` (copies docs/screenshots, social preview, icon into img/) -> https://ripdzl.github.io/Kraken-Control/ (Pages source: GitHub Actions). Download links resolve the latest release zip via the GitHub API at runtime.
- Releases: push tag `v<Cargo version>` -> `.github/workflows/release.yml` (windows-latest: tests, `package.ps1`, SHA-256) -> DRAFT release with zip + .sha256; notes from `.github/release-notes.md` (edit "What's new", then Publish). Tag must equal Cargo.toml version.
- Tester package: `packaging\package.cmd` -> `dist\KrakenControl-<ver>-portable.zip` (static CRT build in `target\portable`, `portable` marker, README-TESTERS.txt). `/dist` gitignored.

## UI / themes (2026-11-08)
- Themes: Dark Grey (default) + Black-Plumb (user's theme file: window #46335a, base.alt #000, base.bg #586c72, button #853894, highlight #7ac143 + black text, bright #ffc30f, divider/error #df503b, link #ffff00) + White (light; `Palette.dark = false` -> egui light visuals) + Colour-blind (2026-11-14: Okabe–Ito accent #56B4E9 / unsaved #F0E442 / error #CC79A7 / key_tag #F5A623, `select_width` 3.5 vs `tag_width` 1.5). Stored in state.json `settings.theme`. Tests: `all_text_is_readable` (every theme, WCAG), `colour_blind_roles_stay_apart` (Machado 2009 protan / deutan / tritan, OKLab distance >= 0.09). Travel-value colours are per palette (`Palette.travel`). UI text never names a colour (themes differ).
- Fonts: Atkinson Hyperlegible Next (Dark Grey, White, Colour-blind), JetBrains Mono (Black-Plumb, user's choice); Latin subsets ~180 KB total, OFL licences shipped in zip. egui default fonts NOT built in (eframe without `default_fonts`); symbols via KrakenArrows.ttf / KrakenStar.ttf fallbacks — add glyphs there (fontTools subset) if new symbols appear in UI text.
- Every UI colour comes from `theme::pal()`; never hard-code Color32 for chrome (data colours like key RGB are fine).
- Pages: Performance, Keymap, Advanced keys, Lighting, Profiles, Settings (same order both boards). Wording: "profile" (not slot), plain helper line under each setting.
- Layout pattern: keyboard card + legend line, then cards of `theme::setting` rows (label + helper left, control right); `theme::section_with` for a title with buttons on the right; one accent `primary_button` per editor (Create / Apply). Performance = "Selected keys" + "Whole keyboard" cards side by side above 860 px (both boards). Gen-2 Whole keyboard = anti-shake, Adaptive Stable, live key depth; Settings keeps polling, tick rate, key locks.
