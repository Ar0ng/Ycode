param([switch]$Install)
$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot
$env:CARGO_NET_OFFLINE = 'true'
$env:VITE_INTERNAL_PREVIEW = 'true'
npm run tauri -- build --config src-tauri/tauri.preview.conf.json --features internal-preview
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
if ($Install) {
    $entry = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\YC_pre' -ErrorAction Stop
    $dir = $entry.InstallLocation.Trim('"')
    if (-not (Test-Path -LiteralPath (Join-Path $dir 'YC_pre.exe'))) { throw 'YC_pre installation not found; install the preview package first.' }
    if (Get-Process -Name YC_pre -ErrorAction SilentlyContinue) { throw 'Close YC_pre before updating it.' }
    $installer = (Resolve-Path 'src-tauri/target/release/bundle/nsis/YC_pre_0.2.3-preview.1_x64-setup.exe').Path
    $result = Start-Process -FilePath $installer -ArgumentList "/S /UPDATE /D=$dir" -WindowStyle Hidden -Wait -PassThru
    if ($result.ExitCode -ne 0) { throw "YC_pre installer failed: $($result.ExitCode)" }
}
