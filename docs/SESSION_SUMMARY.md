# SESSION_SUMMARY

Current state and handoff. History is in PROGRESS.md.

## State (2026-11-10)
- Kraken Control v0.12.0: native Rust app for DrunkDeer A75 Master/Ultra (gen-1, VID 352D) and A75 Ultra 2025 (gen-2, 19F5:FB3E). Public at github.com/ripDZL/Kraken-Control (`main`), MIT.
- Release v0.12.0 built by `.github/workflows/release.yml` on windows-latest (tests green on Windows), zip 1.6 MB (exe 3.1 MB, static CRT) + .sha256.
- CI (`ci.yml`, Windows, stable Rust 1.99: clippy -D warnings + tests) green on `main`.
- Local folder `drunkdeer-rs` on the user's PC is a git working copy of `main` (upstream origin/main). Commits must use `228260327+ripDZL@users.noreply.github.com` (never the account email; GitHub email privacy is on).

## Workflow
- Cloud: add_repo ripDZL/Kraken-Control, clone, edit, `cargo test` / clippy (cloud cargo 1.97; CI has newer lints), push `main`. User runs `git pull`.
- Session limits: tag pushes refused by the git proxy (HTTP 403); releases and repo settings refused by the API proxy. Tags: user pushes. Releases / settings: user, or the Claude app's built-in browser (signed in to GitHub).
- Release: bump `Cargo.toml` version + CHANGELOG, push, user tags `v<version>` -> draft release -> paste CHANGELOG entry into "What's new" -> Publish.
- Screenshots: `xvfb-run -s "-screen 0 1180x760x24"` + `import -window root`; `APPDATA=<dir>` isolates state; move the pointer away (python-xlib warp) to avoid tooltips.
- Device VM has no Rust; don't run git there (no delete permission -> stale .lock files). Files copied to the PC via device tools get C2PA metadata added to images.

## Decisions
- egui/eframe =0.31.1; software renderer default (winit + softbuffer), GPU (glow) optional + fallback. Raw Win32 HID via windows-sys.
- Gen-1 keymap is not readable: app copy is source of truth, written only when edited. Writes read back where possible; gen-1 lighting is ack-only.
- Never firmware / factory reset / calibration writes. Protocol facts from official driver bundles only.
- Tray agent = separate `--agent` process (no GL), `Local\KrakenControlHid` mutex shared with UI.

## Unverified on hardware / Windows
- v0.12.0 Export / Import with real file dialogs, import into a fresh install, then Save to keyboard (both boards).
- Software renderer checks (DPI, resize, clipboard, minimise), memory vs GPU.
- Gen-1 Lighting (v0.9) on hardware; gen-2 Toggle add/remove; tray agent end-to-end.

## Next
1. Users test the v0.12.0 zip; triage issues.
2. UI overhaul remainder: gen-2 Performance two-card layout; restyle Keymap / Advanced / Lighting editors.
3. Optional: release workflow fills "What's new" from CHANGELOG automatically.
