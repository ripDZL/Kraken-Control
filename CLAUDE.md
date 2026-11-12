# drunkdeer-rs (Kraken Control) — agent rules

- Read `docs/AI_CONTEXT.md`, `docs/TODO.md`, `docs/SESSION_SUMMARY.md` first.
- Protocol facts come from the official driver bundle only (`docs/protocol-a75-master-ultra.md`); cite line refs. Never copy bundle code.
- `dd-proto` stays pure (no I/O). Add a golden test for every new packet builder/parser.
- Verify: `cargo test --workspace`; `cargo clippy --workspace --all-targets -- -D warnings` (CI runs both on Windows with newer stable Rust); `cargo check -p kraken-control --features check-win` type-checks Win32 HID on non-Windows hosts.
- UI screenshots on Linux: `xvfb-run` + `kraken-control --demo [--tab=performance|keymap|advanced|lighting|profiles|settings]` (gen-2: `--demo-gen2 --g2tab=...`; `--g2draft=dks|rs|oks|macro` opens an editor, `--import=<file>` opens the import dialog), `--theme=plumb` for Black-Plumb (not saved).
- Colours only via `theme::pal()`; fonts in `app/res/fonts` (OFL).
- Source of truth: github.com/ripDZL/Kraken-Control `main`. Cloud: clone, build/clippy/test/screenshot, push `main`; user runs `git pull` (see SESSION_SUMMARY Workflow). Commit as `228260327+ripDZL@users.noreply.github.com`. Device VM has no Rust and must not run git.
- App icon: `app/res/kraken.png` -> `app/res/kraken.ico` (regenerate with a script; 16–64 px DIB + 256 PNG). `app/build.rs` writes the .res itself (no rc.exe); output was verified byte-identical to llvm-rc.
