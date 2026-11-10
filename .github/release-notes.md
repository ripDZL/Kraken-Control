Portable build for Windows 10 and 11 (64-bit). Nothing to install, no .NET, no WebView.

## What's new

<!-- Edit before publishing. -->

## Download and run

1. Download **KrakenControl-{{VERSION}}-portable.zip** below and unzip it somewhere you can write to (Desktop or Documents, not Program Files).
2. Close the DrunkDeer web driver tab and any other keyboard app.
3. Run `KrakenControl\kraken-control.exe`. The exe isn't code-signed, so Windows SmartScreen may warn the first time: click **More info**, then **Run anyway**.

Settings and the log are kept in the `data` folder next to the exe. To remove Kraken Control, turn off "Start with Windows" if you enabled it, then delete the folder.

## Supported keyboards

- DrunkDeer A75 Master and A75 Ultra (original, USB ID 352D)
- DrunkDeer A75 Ultra 2025 (USB ID 19F5:FB3E)

## Safety

Kraken Control never updates firmware, never factory-resets and never calibrates. Lighting changes apply straight away; everything else is written only when you press **Save to keyboard**.

## Check the download

SHA-256 of the zip:

```
{{SHA256}}
```

In PowerShell: `Get-FileHash .\KrakenControl-{{VERSION}}-portable.zip`

Built from this tag by GitHub Actions on a Windows runner.
