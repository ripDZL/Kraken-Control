# SESSION_SUMMARY

- Objective: **Kraken Control** (renamed from Deer Control) — native Rust replacement for DrunkDeer-Control (user's A75 Master fw 85). Order: 1 diag log, 2 LW/RDT, 3 RT Match, 4 onboard slots, 5 tray/autostart/per-app, 6 lighting/knob/key depth, 7 performance pass (RAM < ~100 MB).
- State: gen-1 features 1–5 done (v0.3; tray untested). Gen-2 (2025 A75 Ultra, VID 19F5 PID FB3E): v0.4 confirmed on hardware (read, travel, settings); v0.5 (remap, colours, lighting) confirmed; v0.6 verified (knob, SOCD add/remove + priority, Mod Tap); Toggle untested. v0.7 (written, NOT compiled — no Rust/crates on device VM, no staging tool): Lighting drag paint + live apply. Next: 6 (needs RE of AE 01 lighting, C1 03 knob, FD 03/FD 06 depth from `scratchpad/work/pretty.js` — re-extract bundle if scratchpad gone: user zip "New folder (21).zip").
- Decisions: egui glow, windows-sys raw HID; keymap written only when edited; actuation + LW verified by read-back; keyboard is source of truth for slots; slot switch blocked while unsaved edits; Turbo on clears LW/RDT. Tray = separate `--agent` process (no GL) so background RAM stays tiny; UI and agent coordinate via `Local\KrakenControlHid` mutex; per-app config in `agent.json` (UI + agent both read-modify-write; agent reloads on mtime).
- Build/test: `cargo test` dd-proto 17 + app 4 (incl. icon orientation); `cargo check/clippy -p kraken-control --features check-win` clean; Linux clippy clean.
- Risks: RDT read-back order inferred; per-slot keymaps unknown; exe .res link via `rustc-link-arg-bins` unverified on MSVC (path has a space); legacy migration untested.
- Next step: user tests v0.3 (TODO P0), then task 6.
- Gen-2: separate stack (dd-proto::gen2, device2.rs, ui2.rs); App shows one board at a time (`Board` enum, auto-follows connection). Unverified: reply byte-1 echo, A1 notification layout, whether replies can start with A1/A2 (treated as notifications). Bundle at `Drunk Deer/gen2-bundle/` (user's machine), prettified copy in scratchpad `gen2/pretty/` (lost if session ends: re-stage + `prettier`).
- Next (2026-11-08): user builds v0.8.1 (Stripes/Checker/palettes). Per-key colours inside firmware effects + animated effects both need `gen2-bundle` access. Animated effects blocked on access to `Drunk Deer/gen2-bundle` (search for live/RAM-only RGB command, e.g. music/screen sync); auto-mode denied the folder request — user must add folder. Then decide custom-effect approach (TODO P1).
- Old next: user tests v0.4 with packet log; then gen-2 remap/knob/advanced keys, tray for gen-2, gen-1 task 6, performance pass.

## Paused 2026-11-08 (session end)
- Done this session: v0.6 verified (knob, SOCD + priority, Mod Tap); v0.7/0.7.1 live lighting + drag paint (user OK); v0.8 Highlight/Pattern (user OK); v0.8.1 Stripes/Checker/palettes (uncompiled); v0.8.2 portable mode + packaging (uncompiled).
- Files changed: app/src/ui2.rs, app/src/store.rs, app/src/main.rs, crates/dd-proto/src/pattern.rs (new, lib.rs `pub mod pattern`), packaging/*, Cargo.toml version, .gitignore, docs/*.
- Build/test: v0.8 compiled + ran on Windows. v0.8.1/0.8.2 NOT compiled (no Rust/crates on device VM; no staging tool to cloud). pattern.rs tested standalone in cloud: 8/8 + clippy clean.
- Tooling notes: device VM (device_bash) has no cargo, crates.io 403. Cloud container has cargo 1.97 + crates.io. Transfer = base64 gzip through tool calls (fine for single files, too big for whole repo ~60 KB xz).
- Blocked: per-key colours inside firmware effects + animated effects need `Drunk Deer\gen2-bundle` (folder request denied by auto mode; user must add folder via "+" > Add folder). Search bundle for func [12] values and any non-saving/live RGB command.
- Immediate next step: user runs `packaging\package.cmd`; fix compile errors; test v0.8.1 patterns + portable zip on another PC.
- 2026-11-08 later: friend (A75 Master) tested the portable zip (so v0.8.2 compiles). Gen-1 bundle re-extracted in cloud scratchpad (`g1/p140.js`, lost with session; source zip = user's "New folder (21).zip", attach again if needed). v0.9 adds gen-1 Lighting (uncompiled). Next: user builds, friend tests effects + custom per-key; watch log for "Key colours sent" / errors.
- DONE (v0.10.0, uncompiled): UI overhaul phase 1 — see PROGRESS 2026-11-08 UI overhaul.
- Earlier note: UI overhaul — better looking, easy for anyone (both boards: gen-1 ui.rs, gen-2 ui2.rs). Start by agreeing a design direction (plain-language labels, help text, grouping, consistent tabs across boards). Ground it in real visuals: user screenshots of every tab, or copy source to cloud and use `--demo` / `--demo-gen2` + `--tab=` / `--g2tab=` under xvfb (CLAUDE.md) so changes compile and can be screenshotted before the user builds.
- v0.9 status at pause: unbuilt by user (gen-1 Lighting). Build first; fix errors before UI work.


## 2026-11-08 UI overhaul session
- Objective: themed, plain-language UI for both boards. Themes: Dark Grey (default), Black-Plumb (user's theme JSON mapped to roles; mapping table in mock canvas).
- Done: theme.rs + shell + gen-1 Performance/Profiles/Settings restructure + gen-2 shell hookup (v0.10.0). Files: app/src/{theme.rs (new), ui.rs, ui2.rs, main.rs, store.rs}, app/res/fonts/* (new), Cargo.toml, packaging/package.ps1, docs/*.
- Build/test: theme.rs compiled + rendered in cloud scratch crate (eframe =0.31.1, xvfb) — both themes OK. ui.rs/ui2.rs/main.rs/store.rs edits NOT compiled (device VM has no Rust; installing a toolchain there was refused by the auto-mode classifier). v0.9 gen-1 Lighting also still unbuilt.
- Tooling: device VM reaches npm, PyPI (fontTools preinstalled), index/static.crates.io, conda-forge; NOT static.rust-lang.org. Cloud container has cargo 1.97 + xvfb. Same-content proof: md5 of theme.rs matched on both sides.
- Unverified UI risks: nested ScrollArea avoided by not wrapping Keymap/Advanced (gen-1) and Keymap (gen-2) pages in the page scroll; keyboard unit now also limited by 55 % of available height.
- Immediate next step: user runs `packaging\package.cmd` (or `cargo build --release`), sends errors; fix; then screenshots of every page in both themes.
- Next phases: gen-2 Performance in the same Selected keys / Whole keyboard layout; Keymap, Advanced, Lighting editors restyled (setting rows, helper text, chip pickers); light theme optional.
- v0.10.0 compiled and ran on Windows. v0.10.1 written (layout feedback): see PROGRESS. Next: user builds v0.10.1, sends screenshots of every page in both themes.

## 2026-11-10
- Done: v0.10.2 = same keyboard size on every page + lighting presets (see PROGRESS 2026-11-10). Files: app/src/{theme.rs, store.rs, ui.rs, ui2.rs}, Cargo.toml, docs/*, CLAUDE.md.
- Build/test: compiled, clippy-clean (also check-win), tests pass, screenshots + click tests in cloud. Not yet built on Windows by user.
- Tooling (new, works): `device_bash` tar of repo (no target/dist) into `target\kc-src.tar.xz` -> `device_stage_files` -> cloud `/mnt/user-data/uploads/drunkdeer-rs/target/kc-src.tar.xz` -> build in `/home/claude/kc`; changed files back via `/mnt/user-data/outputs/` + `device_commit_files`. Screenshots: `xvfb-run -s "-screen 0 1180x760x24"` + `import -window root`; `APPDATA=<dir>` isolates state.json; clicks via python-xlib XTest (set input focus on the "Kraken" window first or typing is lost).
- Immediate next step: user builds v0.10.2 (`packaging\package.cmd`), checks presets on both boards with hardware.
- Next phases (TODO UI overhaul): gen-2 Performance two-card layout; restyle Keymap / Advanced / Lighting editors.
- v0.10.3 memory pass done (see PROGRESS). Next: user builds, reads `memory:` line in log / Settings > Troubleshooting on Windows; decide on software renderer.
- v0.11.0 (see PROGRESS): software renderer default, page alignment, contrast tests. Next: user builds on Windows, compares Task Manager / Process Explorer and the Settings memory line for Processor vs Graphics card; report frame ms on a maximised window.
- v0.12.0: profile export / import (see PROGRESS). Next: user builds on Windows; test Export / Import dialogs, import on a fresh %APPDATA% (or portable copy), then Save to keyboard.
