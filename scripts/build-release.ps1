# Signed release build (NSIS installer + updater signature).
# Usage: powershell -ExecutionPolicy Bypass -File scripts\build-release.ps1 [-Log path]
param([string]$Log = "")
$ErrorActionPreference = "Continue"
$root = Split-Path -Parent $PSScriptRoot
if (-not $env:RUSTUP_HOME -and (Test-Path 'D:\DevTools\rust\rustup')) { $env:RUSTUP_HOME = 'D:\DevTools\rust\rustup'; $env:CARGO_HOME = 'D:\DevTools\rust\cargo' }
if ($env:CARGO_HOME) { $env:Path = "$env:CARGO_HOME\bin;" + $env:Path }
$keyDir = Join-Path $env:USERPROFILE ".ycode-signing"
if (-not $env:TAURI_SIGNING_PRIVATE_KEY -and (Test-Path "$keyDir\ycode.key")) {
  $env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$keyDir\ycode.key" -Raw
  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = Get-Content "$keyDir\password.txt" -Raw
}
Set-Location $root
$sw = [Diagnostics.Stopwatch]::StartNew()
if ($Log) {
  npm run tauri build *>&1 | Out-File -FilePath $Log -Encoding utf8
  "BUILD_SECONDS=$([int]$sw.Elapsed.TotalSeconds) EXIT=$LASTEXITCODE" | Out-File -FilePath $Log -Append -Encoding utf8
} else {
  npm run tauri build
}
