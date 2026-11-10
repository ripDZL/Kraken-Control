# Security policy

## Reporting a problem

Please report security problems privately, not as a public issue: use
**Report a vulnerability** on this repository's Security tab.

## Scope

Kraken Control talks to DrunkDeer keyboards over USB HID and stores its
settings in `%APPDATA%\KrakenControl` (or `data\` next to a portable exe). It
makes no network connections. Relevant problems include anything that lets a
crafted settings or profile file, or a misbehaving device, make Kraken Control
send unintended commands to a keyboard, crash, or write outside its settings
folder.

Only the latest release gets fixes.
