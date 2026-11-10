# Builds dist\KrakenControl-<version>-portable.zip for testers:
#   KrakenControl\kraken-control.exe  (static C runtime: no VC++ redistributable needed)
#   KrakenControl\portable            (marker: settings + log go to .\data, not %APPDATA%)
#   KrakenControl\README-TESTERS.txt
#   KrakenControl\licenses\*.txt      (embedded fonts)
# Run: packaging\package.cmd (or: powershell -ExecutionPolicy Bypass -File packaging\package.ps1)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    $env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
}
$ver = (Select-String -Path Cargo.toml -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value

cargo test -q -p dd-proto
if ($LASTEXITCODE -ne 0) { throw "dd-proto tests failed" }

# Static CRT, own target dir so normal builds are not invalidated.
$env:RUSTFLAGS = '-C target-feature=+crt-static'
cargo build --release -p kraken-control --target-dir target\portable
$code = $LASTEXITCODE
Remove-Item Env:RUSTFLAGS
if ($code -ne 0) { throw "cargo build failed" }

$name = "KrakenControl-$ver-portable"
$out = Join-Path $root "dist\$name"
$stage = Join-Path $out 'KrakenControl'
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item target\portable\release\kraken-control.exe $stage
Copy-Item packaging\README-TESTERS.txt $stage
# Embedded fonts are OFL-1.1: ship their licences with the binary.
New-Item -ItemType Directory -Force (Join-Path $stage 'licenses') | Out-Null
Copy-Item app\res\fonts\*.txt (Join-Path $stage 'licenses')
Set-Content -Path (Join-Path $stage 'portable') -Value 'Portable copy: settings and log are kept in the data folder next to kraken-control.exe.'
$zip = Join-Path $root "dist\$name.zip"
Compress-Archive -Path $stage -DestinationPath $zip -Force
Write-Host "Done: $zip"
