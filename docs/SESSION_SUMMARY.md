# SESSION_SUMMARY

Current state and handoff. History is in PROGRESS.md.

## State (2026-11-11)
- Kraken Control v0.12.0: native Rust app for DrunkDeer A75 Master/Ultra (gen-1, VID 352D) and A75 Ultra 2025 (gen-2, 19F5:FB3E). Public at github.com/ripDZL/Kraken-Control (`main`), MIT.
- Release v0.12.0 published (Latest), built by `.github/workflows/release.yml` on windows-latest (tests green on Windows), zip 1.6 MB (exe 3.1 MB, static CRT) + .sha256. Repo: topics set, private vulnerability reporting on, community files incl. CODE_OF_CONDUCT; social preview = `docs/social-preview.png`.
- Website live at https://ripdzl.github.io/Kraken-Control/ from `site/` (pages.yml). Light page + app Dark Grey surfaces, teal accent. Hero demo tabs: Simulate (drag keycap, gen-2 ranges) and Your keyboard (live keydown/keyup timeline; browsers can't read depth).
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
- Gen-2 DKS / Rappy Snappy / One Key Switch editors and macros (2026-11-11): user ran build a655f15 on the 2025 A75 Ultra, "everything seems okay" (not itemised, no log). Check list in TODO "Web-driver parity".

## Next
1. User tests on hardware: gen-1 live key depth (Performance) and knob (Keymap), then the v0.12.0 zip items still open.
2. Web-driver parity, gen-2 (A75 Ultra 2025): the planned list is DONE on `main` (unreleased): DKS / Rappy Snappy / OKS editors, macros, light direction, live view, import of the official profile export. User reported DKS / RS / OKS / macros OK on hardware in general terms, and Live view working (then sped up, 2026-11-12, untested again); Direction buttons and official import untested. Open check lists: TODO "Web-driver parity". Gen-2 live key depth built 2026-11-13 via the debug bit (func[7] b3), hardware untested. Not applicable: light point (type 102 only), sleep time (not in the bundle).
   - 2026-11-12: user confirmed Live view (after speed-up: not re-confirmed) and official import / export working on hardware. Version bumped to 0.13.0 on `main`, CHANGELOG section dated; tag `v0.13.0` pushed by the user (31878ba), Release workflow built the DRAFT (zip + .sha256). Left for the user: paste the CHANGELOG 0.13.0 entry into "What's new", Publish (this session can't edit or publish releases: API 403 / auto-mode refusal). Parked: slow memory growth on Windows (TODO).
   - Local branch `dev` in the cloud clone (2026-11-13; user: no repo updates / no release until more changes are in): FN-layer advanced keys + macro keys. NOT pushed; push / merge to `main` + version bump only when the user says. If the session ends, the work is lost unless pushed.
   - Left from the inventory: macro names, other gen-2 models' layouts.
3. Gen-1 bundle: ask the user to attach "New folder (21).zip" (gen-1 only, = live site, SHA-256 `0aa5b587f453d1b8…`), or read it in the built-in browser (`fetch('/js/index.exKUY8vl.js')` on beta.drunkdeer-antler.com); shells can't reach the site. Gen-2 bundle is only in `…\Drunk Deer\gen2-bundle\`.
4. UI overhaul remainder: gen-2 Performance two-card layout; restyle Keymap / Advanced / Lighting editors.
