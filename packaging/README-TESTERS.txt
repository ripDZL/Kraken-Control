Kraken Control (portable)
=========================

What it is
  A small Windows app for DrunkDeer keyboards, replacing the web driver.
  Supported:
    - A75 Master / A75 Ultra (older models, USB ID 352D)
    - A75 Ultra 2025 (USB ID 19F5:FB3E)
  Other DrunkDeer models are detected but not supported yet.
  Knob and advanced keys: A75 Ultra 2025 only for now. The older A75 Master /
  Ultra get key travel, remap, Last Win / RDT, lighting,
  profiles and tray. Their lighting can't be read from the keyboard, so the
  Lighting tab shows what the app last sent.

Requirements
  Windows 10 or 11, 64-bit. Drawing is done on the CPU by default; Settings >
  Drawing can switch to the graphics card (OpenGL).

How to run
  1. Unzip the KrakenControl folder somewhere you can write to (Desktop or
     Documents; not Program Files).
  2. Close the DrunkDeer web driver tab in your browser first.
  3. Start kraken-control.exe. This test build is unsigned, so Windows
     SmartScreen may warn: click "More info", then "Run anyway".

Portable
  Settings and the log are kept in the "data" folder next to the exe.
  Nothing is installed. To remove it, delete the folder. If you turned on
  "Start with Windows" for the tray icon, turn it off first.

Safety
  - Never updates firmware, never factory-resets, never calibrates.
  - Writes to the keyboard only when you press "Save to keyboard", or
    straight away on the Lighting tab.
  - Writes are read back from the keyboard and checked where the keyboard
    allows it (older A75 lighting and keymaps can't be read back).
  Take screenshots of your current settings first so you can put them back.

Reporting problems
  Open an issue at https://github.com/ripDZL/Kraken-Control/issues and
  attach data\log.txt with a short note: what you did, what you expected,
  what happened. If asked, tick "Log packets (hex)" and repeat the steps.
