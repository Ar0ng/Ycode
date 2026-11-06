# Local pre-release build of the separate "YC_pre" app (snapshot / preview / rc).
#
#   powershell -ExecutionPolicy Bypass -File scripts\build-pre.ps1 -Channel preview -Number 2 -Install
#
# - Version = <package.json version base>-<channel>.<number>, e.g. 0.2.3-preview.2
#   (pass -Base to choose the base, default: next patch after package.json's version).
# - Uses src-tauri/tauri.pre.conf.json: product "YC_pre", identifier
#   com.ycode.desktop.preview (own data folder), no updater artifacts, unsigned.
# - Compiles with YCODE_CHANNEL=<channel>, so the app never updates itself from GitHub.
# - -Install runs the installer silently into -InstallDir (default E:\YCode\YC_pre).
# - Nothing in the repo is modified; nothing is pushed or published.
#
# Run it from a normal terminal (not from inside the Claude desktop app's sandbox,
# see docs/BUILD.md).
param(
  [Parameter(Mandatory)][ValidateSet("snapshot", "preview", "rc")][string]$Channel,
  [Parameter(Mandatory)][ValidateRange(1, 999)][int]$Number,
  [string]$Base = "",
  [switch]$Install,
  [string]$InstallDir = "E:\YCode\YC_pre",
  [string]$Log = ""
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $env:RUSTUP_HOME -and (Test-Path 'D:\DevTools\rust\rustup')) { $env:RUSTUP_HOME = 'D:\DevTools\rust\rustup'; $env:CARGO_HOME = 'D:\DevTools\rust\cargo' }
if ($env:CARGO_HOME) { $env:Path = "$env:CARGO_HOME\bin;" + $env:Path }
Set-Location $root

function Say($m) { if ($Log) { $m | Out-File -FilePath $Log -Append -Encoding utf8 } else { Write-Host $m } }

if (-not $Base) {
  $cur = (Get-Content package.json -Raw | ConvertFrom-Json).version -replace '-.*$', ''
  $p = $cur.Split('.')
  $Base = "{0}.{1}.{2}" -f $p[0], $p[1], ([int]$p[2] + 1)
}
if ($Base -notmatch '^\d+\.\d+\.\d+$') { throw "Base must look like 0.2.3" }
$version = "$Base-$Channel.$Number"

# Merge the pre-release overlay with the version into one temporary config file.
$conf = Get-Content src-tauri\tauri.pre.conf.json -Raw | ConvertFrom-Json
$conf.PSObject.Properties.Remove('$comment')
$conf | Add-Member -NotePropertyName version -NotePropertyValue $version -Force
$tmpDir = Join-Path $root "src-tauri\target"
New-Item -ItemType Directory -Force $tmpDir | Out-Null
$tmpConf = Join-Path $tmpDir "pre.conf.json"
[IO.File]::WriteAllText($tmpConf, ($conf | ConvertTo-Json -Depth 10))

$env:YCODE_CHANNEL = $Channel
# Same switches as the 0.2.3-preview.1 build (build-preview.ps1): preview logo and separate
# "YC_pre" API keys in Credential Manager (falls back to the stable "Ycode" keys).
$env:VITE_INTERNAL_PREVIEW = 'true'
Remove-Item Env:TAURI_SIGNING_PRIVATE_KEY, Env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD -ErrorAction SilentlyContinue

Say "Building YC_pre $version ($Channel)"
$sw = [Diagnostics.Stopwatch]::StartNew()
$ErrorActionPreference = "Continue"
if ($Log) { npm run tauri build -- --config $tmpConf --features internal-preview *>&1 | Out-File -FilePath $Log -Append -Encoding utf8 } else { npm run tauri build -- --config $tmpConf --features internal-preview }
$code = $LASTEXITCODE
$ErrorActionPreference = "Stop"
$setup = Join-Path $root "src-tauri\target\release\bundle\nsis\YC_pre_${version}_x64-setup.exe"
Say "BUILD_SECONDS=$([int]$sw.Elapsed.TotalSeconds) EXIT=$code"
if ($code -ne 0 -or -not (Test-Path $setup)) { Say "BUILD FAILED"; exit 1 }
Say "SETUP=$setup"

if ($Install) {
  Say "Installing into $InstallDir"
  # /D must be last and unquoted (NSIS).
  $p = Start-Process $setup -ArgumentList "/S /D=$InstallDir" -PassThru -Wait
  $exe = Join-Path $InstallDir "YC_pre.exe"
  $installed = if (Test-Path $exe) { (Get-Item $exe).VersionInfo.ProductVersion } else { "missing" }
  Say "INSTALL_EXIT=$($p.ExitCode) INSTALLED=$installed"
  if ($p.ExitCode -ne 0 -or $installed -ne $version) { exit 1 }
}
Say "DONE"
