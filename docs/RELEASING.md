# Releasing & auto-update

Ycode updates through **GitHub Releases + the official Tauri updater**. Every
update package is signed with a minisign key; the app only installs a package
whose signature matches the public key compiled into it (`plugins.updater.pubkey`
in `src-tauri/tauri.conf.json`). `requireSignedVersion` is on, so a manifest
can't pair a newer version number with an older signed installer (downgrade
protection).

## One-time setup

1. **Repository:** https://github.com/Ar0ng/Ycode
2. **Signing key.** A key pair was generated for this project on the dev machine:
   - private key: `%USERPROFILE%\.ycode-signing\ycode.key` (never commit it)
   - password: `%USERPROFILE%\.ycode-signing\password.txt`
   - public key: already in `tauri.conf.json`

   Back both files up somewhere safe (password manager). **If the private key or
   its password is lost, installed copies can never be updated again** — you would
   have to ship a new key in a manually installed build.

   To rotate or create your own key instead:
   ```bash
   npx tauri signer generate -w "$HOME/.ycode-signing/ycode.key"
   ```
   then paste the new `.pub` contents into `plugins.updater.pubkey`.
3. **GitHub secrets** (repo → Settings → Secrets and variables → Actions):

   | Secret | Value |
   |---|---|
   | `TAURI_SIGNING_PRIVATE_KEY` | full contents of `ycode.key` |
   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | contents of `password.txt` |

   `GITHUB_TOKEN` is provided automatically; the workflow requests
   `contents: write` to create the release.
4. **In the app:** the updater checks `Ar0ng/Ycode` by default (Settings → Updates to change it; `DEFAULT_UPDATE_REPO` in `src-tauri/src/storage.rs`).

## Cutting a release

1. Bump the version in `package.json`, `src-tauri/tauri.conf.json` and
   `src-tauri/Cargo.toml` (plain semver, e.g. `0.1.4`; the app displays it as `V0.1.4`),
   then run `cargo update -p ycode` in `src-tauri` so `Cargo.lock` matches — CI and the
   release build use `--locked` and fail otherwise.
2. Commit, then tag and push:
   ```bash
   git tag v0.2.0
   ```
   ```bash
   git push origin main --tags
   ```
3. `.github/workflows/release.yml` runs on `windows-latest`: checks the tag matches
   the app version, **verifies the signing key and password in seconds** (a bad Secret
   fails here with a clear message), runs type-check + frontend tests + Rust tests,
   builds and signs the NSIS installer, writes `latest.json` and publishes a
   **non-draft, non-prerelease** release with the `gh` CLI. Failures show up as public
   error annotations on the run (the build failure annotation contains the last 60
   lines of build output), so no sign-in is needed to diagnose them. The release contains:
   - `Ycode_<version>_x64-setup.exe` (installer)
   - `Ycode_<version>_x64-setup.exe.sig` (signature)
   - `latest.json` (updater manifest)

The app checks `https://github.com/<owner>/<repo>/releases/latest/download/latest.json`.
GitHub's "latest" never points at a pre-release or draft, so beta builds published as
pre-releases are not offered to users.

## What the user sees

- On start (if enabled) a background check runs a few seconds after launch; offline
  or failed checks are silent and never block the app.
- Settings → Updates → *Check for updates* shows the new version and release notes.
- Nothing downloads until the user clicks *Download and install*. The installer runs
  in passive mode and the app restarts.
- Chats, settings and skills live in `%APPDATA%\com.ycode.desktop` and API keys in
  Windows Credential Manager; neither is touched by the installer or uninstaller
  (the NSIS uninstaller only removes app data if the user ticks that option).
- A failed download or signature check leaves the installed version untouched.
- Windows in-app updates target the folder of the running executable, including a
  custom install location. The folder must contain `uninstall.exe`; development
  and portable copies should use a manual install instead. The updater only accepts
  signed NSIS `.exe` update packages. Older installed versions must first receive
  this updater fix before their later updates can preserve the running path.

## Local signed build

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = Get-Content "$env:USERPROFILE\.ycode-signing\ycode.key" -Raw
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = Get-Content "$env:USERPROFILE\.ycode-signing\password.txt" -Raw
npm run tauri build
```
