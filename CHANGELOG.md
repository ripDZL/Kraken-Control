# Changelog

Notable changes to Kraken Control. Versions follow [Semantic Versioning](https://semver.org/).

## [0.13.1] - unreleased

- **White and Colour-blind themes:** Settings › Appearance. White is a light theme. Colour-blind keeps selected, unsaved, error and advanced-key colours apart with any type of colour blindness (blue, yellow, purple and orange from the Okabe–Ito set) and marks selected keys with a thick outline. Changed keys on the Keymap page (2025 A75 Ultra) are now outlined, like on the original boards.

- **Cleaner pages (A75 Ultra 2025):** Performance has the same Selected keys / Whole keyboard cards as the original A75 boards; anti-shake level and Adaptive Stable moved there from Settings. Keymap, Advanced keys (with each editor in its own card), macros, Lighting (Custom colours tools in their own card) and Settings use labelled rows with a short explanation under each option.

- **Macro names (A75 Ultra 2025):** give macros a name in the macro editor. Names are kept on this PC (the keyboard stores macros without names) and are included in Kraken's back-up file.

- **Advanced keys and macros on FN layers (A75 Ultra 2025):** Advanced keys has a Main / FN1 / FN2 / FN3 selector; new advanced keys and macro keys go on the layer shown, like the official driver. The list shows every layer.

- **Live key depth (A75 Ultra 2025):** Performance › Show live key depth shades each key as you press it and marks its actuation point, as the official driver's Performance tab does.

## [0.13.0] - 2026-11-12

- **Import from the official driver (A75 Ultra 2025):** Import from file also takes a profile exported by the official web driver: key travel, keymap and advanced keys, with the same checks as Kraken's own files.
- **Lighting (A75 Ultra 2025):** Direction for Wave, Up and down waves, Fountain and Spin. A Live view under the lighting controls shows the colours the keys have right now while a built-in effect runs.
- **Macros (A75 Ultra 2025):** Advanced keys › Macros. Record your typing or build a macro step by step (press / release, key, pause), then put it on a key. Up to 16 macros per profile, stored on the keyboard; included in export / import.
- **Advanced keys (A75 Ultra 2025):** new editors for Rappy Snappy and One Key Switch pairs and for DKS (Dynamic Keystroke): up to four actions on one key, each tapped or held between four depths of the stroke. DKS keys made in the official driver can be edited, and are included in export / import.
- **Live key depth (original A75 Master / Ultra, not the 2025 model):** Performance › Show live key depth shades each key as you press it on your keyboard, and marks its actuation point. Needs Turbo off, like the official driver.
- **Knob (original A75 Master / Ultra, firmware 0.063+):** Keymap › click the knob to set Rotate Right / Left, Press, Lift and Press + Rotate actions. Saved with the keymap and included in export / import.

## [0.12.0] - 2026-11-10

First public release.

- **Both A75 generations:** per-key actuation and Rapid Trigger, remapping,
  lighting with one-click presets, onboard profiles.
- **Move to another PC:** Profiles › Move to another PC exports every setting
  to one file and imports it on a fresh install. Imported values are checked
  before anything reaches the keyboard.
- **Light on memory:** drawing is done on the CPU by default, so no graphics
  driver is loaded (Settings › Drawing to switch).
- **Two themes:** Dark Grey (default) and Black-Plumb.

Earlier versions were private test builds.

[0.12.0]: https://github.com/ripDZL/Kraken-Control/releases/tag/v0.12.0
