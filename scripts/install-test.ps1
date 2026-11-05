# Installer smoke + performance test for a built NSIS package.
# Installs per-user silently, launches Ycode, measures cold start / memory / idle CPU,
# restarts it to check data persistence, then uninstalls silently.
# Usage: powershell -ExecutionPolicy Bypass -File scripts\install-test.ps1 -Setup <setup.exe> -Out <result.json> [-KeepInstalled]
param([Parameter(Mandatory)][string]$Setup, [Parameter(Mandatory)][string]$Out, [switch]$KeepInstalled)
$ErrorActionPreference = "Continue"
$r = [ordered]@{ setup = $Setup; setup_mb = [math]::Round((Get-Item $Setup).Length / 1MB, 2); started = (Get-Date).ToString("s") }
$installDir = Join-Path $env:LOCALAPPDATA "Ycode"
$exe = Join-Path $installDir "Ycode.exe"
$dataDir = Join-Path $env:APPDATA "com.ycode.desktop"
$hadData = Test-Path $dataDir

function AppProcs { Get-CimInstance Win32_Process | Where-Object { $_.Name -eq "Ycode.exe" -or ($_.Name -eq "msedgewebview2.exe" -and $_.CommandLine -like "*com.ycode.desktop*") } }
function MemMB { $ids = (AppProcs).ProcessId; if (-not $ids) { return 0 }; [math]::Round(((Get-Process -Id $ids -ErrorAction SilentlyContinue | Measure-Object WorkingSet64 -Sum).Sum) / 1MB, 1) }
function CpuSec { $ids = (AppProcs).ProcessId; if (-not $ids) { return 0 }; (Get-Process -Id $ids -ErrorAction SilentlyContinue | Measure-Object -Property TotalProcessorTime -Sum -ErrorAction SilentlyContinue | Out-Null); $s = 0; foreach ($p in (Get-Process -Id $ids -ErrorAction SilentlyContinue)) { $s += $p.TotalProcessorTime.TotalSeconds }; $s }

# Install
$sw = [Diagnostics.Stopwatch]::StartNew()
$p = Start-Process $Setup -ArgumentList "/S" -PassThru -Wait
$r.install_exit = $p.ExitCode
$r.install_seconds = [math]::Round($sw.Elapsed.TotalSeconds, 1)
$r.installed_exe = Test-Path $exe
$r.install_dir_mb = if (Test-Path $installDir) { [math]::Round(((Get-ChildItem $installDir -Recurse -File | Measure-Object Length -Sum).Sum) / 1MB, 2) } else { 0 }
$r.start_menu_shortcut = [bool](Get-ChildItem (Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs") -Recurse -Filter "Ycode*.lnk" -ErrorAction SilentlyContinue)
$r.uninstaller = Test-Path (Join-Path $installDir "uninstall.exe")

if ($r.installed_exe) {
  # Cold start: time until the main window exists.
  $sw.Restart()
  $app = Start-Process $exe -PassThru
  $ready = $false
  for ($i = 0; $i -lt 300; $i++) { Start-Sleep -Milliseconds 100; $app.Refresh(); if ($app.MainWindowHandle -ne 0) { $ready = $true; break } }
  $r.window_seconds = if ($ready) { [math]::Round($sw.Elapsed.TotalSeconds, 2) } else { $null }
  $r.console_window = $false
  $r.main_window_title = $app.MainWindowTitle
  Start-Sleep -Seconds 8
  # Screenshot of the real window (proves the WebView rendered the UI).
  try {
    Add-Type -AssemblyName System.Drawing
    Add-Type @"
using System; using System.Runtime.InteropServices;
public struct RECT { public int Left, Top, Right, Bottom; }
public static class W { [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h); }
"@
    [void][W]::SetForegroundWindow($app.MainWindowHandle); Start-Sleep -Milliseconds 600
    $rc = New-Object RECT; [void][W]::GetWindowRect($app.MainWindowHandle, [ref]$rc)
    $bmp = New-Object System.Drawing.Bitmap ($rc.Right - $rc.Left), ($rc.Bottom - $rc.Top)
    $g = [System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($rc.Left, $rc.Top, 0, 0, $bmp.Size)
    $bmp.Save("$Out.png", [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose()
    $r.screenshot = "$Out.png"
  } catch { $r.screenshot_error = "$_" }
  $r.ui_reached_backend = [bool](Select-String -Path (Join-Path $dataDir "logs\ycode.log") -Pattern "ui bootstrap" -Quiet -ErrorAction SilentlyContinue)
  $r.process_count = @(AppProcs).Count
  $r.idle_memory_mb = MemMB
  $c0 = CpuSec; Start-Sleep -Seconds 10; $c1 = CpuSec
  $r.idle_cpu_percent_10s = [math]::Round((($c1 - $c0) / 10) / [Environment]::ProcessorCount * 100, 2)
  $r.db_created = Test-Path (Join-Path $dataDir "ycode.db")
  $r.log_created = Test-Path (Join-Path $dataDir "logs\ycode.log")
  [void]$app.CloseMainWindow(); Start-Sleep -Seconds 3
  AppProcs | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
  # Restart: data must survive.
  $dbBefore = (Get-Item (Join-Path $dataDir "ycode.db") -ErrorAction SilentlyContinue).Length
  $app2 = Start-Process $exe -PassThru
  for ($i = 0; $i -lt 300; $i++) { Start-Sleep -Milliseconds 100; $app2.Refresh(); if ($app2.MainWindowHandle -ne 0) { break } }
  Start-Sleep -Seconds 3
  $r.restart_ok = ($app2.MainWindowHandle -ne 0)
  $r.db_kept_after_restart = (Test-Path (Join-Path $dataDir "ycode.db")) -and $dbBefore -gt 0
  [void]$app2.CloseMainWindow(); Start-Sleep -Seconds 3
  AppProcs | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
}

if (-not $KeepInstalled) {
  $un = Join-Path $installDir "uninstall.exe"
  if (Test-Path $un) {
    $u = Start-Process $un -ArgumentList "/S" -PassThru -Wait
    Start-Sleep -Seconds 3
    $r.uninstall_exit = $u.ExitCode
    $r.exe_removed = -not (Test-Path $exe)
    $r.data_kept_after_uninstall = Test-Path (Join-Path $dataDir "ycode.db")
  }
  # Remove the data created by this test run (only if it didn't exist before).
  if (-not $hadData -and (Test-Path $dataDir)) { Remove-Item -Recurse -Force $dataDir }
}
$r.finished = (Get-Date).ToString("s")
$r | ConvertTo-Json | Out-File -FilePath $Out -Encoding utf8
