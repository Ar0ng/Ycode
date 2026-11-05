# Windows development & build

## Prerequisites

| Tool | Version used | Notes |
|---|---|---|
| Windows | 10 (supported builds) / 11, x64 | |
| Node.js | 24.x (npm 11) | |
| Rust | stable (1.99.1 at time of writing), `x86_64-pc-windows-msvc` | `rustup` |
| MSVC Build Tools | VS 2022 Build Tools, "Desktop development with C++" + Windows 11 SDK | |
| WebView2 Runtime | preinstalled on Windows 11; the installer bootstraps it on Windows 10 | |

On the original dev machine the toolchain lives on drive D:

```powershell
$env:RUSTUP_HOME = 'D:\DevTools\rust\rustup'
$env:CARGO_HOME  = 'D:\DevTools\rust\cargo'
$env:Path = "D:\DevTools\rust\cargo\bin;" + $env:Path
```
(`RUSTUP_HOME`/`CARGO_HOME` are also stored as user environment variables; new
terminals only need `cargo\bin` on `Path`.) Build Tools are at `D:\DevTools\BuildTools`
and are found automatically through `vswhere`.

## Commands

```bash
npm ci
```
```bash
npm run tauri dev
```
Runs the app with hot reload (Vite on port 1420 + debug Rust build).

```bash
npm run dev
```
UI only, in a normal browser, backed by the fake IPC data in `src/dev/mock.ts`
(development builds only; it is not part of release bundles).

```bash
npm run typecheck
```
```bash
npm test
```
```bash
cd src-tauri && cargo test
```

### Release build

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$env:USERPROFILE\.ycode-signing\ycode.key" -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = Get-Content "$env:USERPROFILE\.ycode-signing\password.txt" -Raw
npm run tauri build
```

Output:
- `src-tauri/target/release/Ycode.exe` — the app (GUI subsystem, no console window)
- `src-tauri/target/release/bundle/nsis/Ycode_<ver>_x64-setup.exe` — installer
- `…-setup.exe.sig` — updater signature (needs the signing key)

Without the signing variables the build fails at the updater-signing step because
`createUpdaterArtifacts` is on. For an unsigned local installer only, run
`npm run tauri build -- --config '{"bundle":{"createUpdaterArtifacts":false}}'`.

## Installer

- NSIS, **per-user** install (`%LOCALAPPDATA%\Ycode`), no admin prompt.
- Start-menu shortcut "Ycode"; English and Simplified Chinese installer UI.
- If WebView2 is missing the installer silently downloads Microsoft's bootstrapper.
- Uninstall: Settings → Apps, or the Start-menu uninstaller. User data in
  `%APPDATA%\com.ycode.desktop` is kept unless the "delete application data" box is
  ticked; API keys stay in Credential Manager until removed in Ycode or Credential Manager.

## Project layout

```
src/                     React + TypeScript UI
  components/            Sidebar, Composer, MessageList, ToolCard, Settings, …
  lib/                   store (zustand), IPC api, i18n, effort helpers, types
  dev/mock.ts            browser-only fake backend (dev builds only)
src-tauri/src/
  agent/                 agent loop, effort planning, context trimming, prompts
  providers/             openai.rs (Responses), deepseek.rs (Chat Completions),
                         anthropic.rs (Messages), registry.rs (capabilities), sse.rs
  tools/                 file/search/edit tools, sandbox.rs (path confinement), shell.rs
  skills.rs              Agent Skills install/validate/progressive loading
  storage.rs             SQLite (WAL) conversations, messages, settings, migrations
  secrets.rs             Windows Credential Manager
  updater.rs             GitHub Releases updater (signed)
  commands.rs            IPC surface
src-tauri/tests/cache_probe.rs   live prompt-cache probe (ignored by default)
examples/skills/         sample skill
docs/                    build, API, skills, releasing, testing, limitations
```
