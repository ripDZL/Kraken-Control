# Contributing to Kraken Control

Bug reports, hardware test results and pull requests are welcome. For anything
bigger than a small fix, open an issue first so we can agree on the approach.

## Build and test

You need Rust (stable, MSVC toolchain) and the Visual Studio Build Tools with
"Desktop development with C++".

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p kraken-control -- --demo        # UI without a keyboard (original A75)
cargo run -p kraken-control -- --demo-gen2   # UI without a keyboard (A75 Ultra 2025)
```

On Linux the UI runs in demo mode only (the HID transport is Windows-only), and
`cargo check -p kraken-control --features check-win` type-checks the Win32 code.
CI runs clippy and the tests on Windows for every pull request.

## Ground rules

- **Never write firmware, factory-reset or calibration commands.** Kraken
  Control must not be able to brick a keyboard. Keyboard functions like these
  are never offered in the key picker either.
- **Read back what you write** wherever the keyboard can report it, and say
  in the UI when it can't.
- **Protocol facts come from the official DrunkDeer drivers**, written up in
  prose in `docs/protocol-*.md` with references. Don't copy driver code.
- `crates/dd-proto` stays pure (no I/O). Every new packet builder or parser
  gets a golden test.
- UI colours come from `theme::pal()` only, so both themes keep readable text
  (the theme tests check WCAG contrast).
- Keep changes focused; avoid unrelated refactors in the same pull request.

## Hardware testing

If you own a model that isn't supported yet, a log with "Log packets (hex)"
ticked (Settings) is the most useful thing you can share. Attach it to an
issue together with the model name and firmware version.
