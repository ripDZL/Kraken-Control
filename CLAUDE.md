# drunkdeer-rs (Kraken Control) — agent rules

- Read `docs/AI_CONTEXT.md`, `docs/TODO.md`, `docs/SESSION_SUMMARY.md` first.
- Protocol facts come from the official driver bundle only (`docs/protocol-a75-master-ultra.md`); cite line refs. Never copy bundle code.
- `dd-proto` stays pure (no I/O). Add a golden test for every new packet builder/parser.
- Verify: `cargo test -p dd-proto`; `cargo check -p kraken-control --features check-win` type-checks Win32 HID on non-Windows hosts.
- UI screenshots on Linux: `xvfb-run` + `kraken-control --demo [--tab=performance|keymap|advanced|lighting|profiles|settings]` (gen-2: `--demo-gen2 --g2tab=...`), `--theme=plumb` for Black-Plumb (not saved).
- Colours only via `theme::pal()`; fonts in `app/res/fonts` (OFL).
- Build in cloud: tar repo (no target/dist) to `target\kc-src.tar.xz` on device, stage it, build/clippy/test/screenshot in cloud, commit changed files back (see SESSION_SUMMARY 2026-11-10). Device VM has no Rust.
- App icon: `app/res/kraken.png` -> `app/res/kraken.ico` (regenerate with a script; 16–64 px DIB + 256 PNG). `app/build.rs` writes the .res itself (no rc.exe); output was verified byte-identical to llvm-rc.
