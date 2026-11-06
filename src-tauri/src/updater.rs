//! GitHub Releases updater built on tauri-plugin-updater (signed artifacts only).
//! The repository is a user setting; the endpoint is the stable "latest" release
//! asset `latest.json`, which GitHub never resolves to a pre-release.

use crate::error::{AppError, AppResult};
use serde::Serialize;
use tauri::{AppHandle, Runtime};
use tauri_plugin_updater::UpdaterExt;

/// Release channel baked in at compile time (`YCODE_CHANNEL`, set by
/// scripts/build-pre.ps1). Only `stable` builds update from GitHub; snapshot,
/// preview and rc builds (the separate "YC_pre" app) are replaced by installing
/// newer local builds, so they can never pull the stable installer into their
/// own folder.
pub const CHANNEL: &str = match option_env!("YCODE_CHANNEL") {
    Some(c) => c,
    None => "stable",
};

pub fn updates_allowed(channel: &str) -> bool {
    channel.is_empty() || channel == "stable"
}

fn ensure_stable() -> AppResult<()> {
    if updates_allowed(CHANNEL) {
        Ok(())
    } else {
        Err(AppError::Update(format!(
            "This is a {CHANNEL} build. Pre-release builds don't update from GitHub; install the next local build instead."
        )))
    }
}

/// NSIS requires /D to be the final argument, unquoted even when it contains spaces.
/// Only target an existing NSIS installation, never a development or portable build.
#[cfg(windows)]
fn nsis_install_dir_arg(exe: &std::path::Path) -> AppResult<std::ffi::OsString> {
    let dir = exe.parent().filter(|p| p.is_absolute() && p.parent().is_some())
        .ok_or_else(|| AppError::Update("Cannot determine the current Ycode installation folder.".into()))?;
    if !dir.join("uninstall.exe").is_file() {
        return Err(AppError::Update("Cannot update this copy automatically: its installation folder has no uninstaller. Run the installer and select the existing Ycode folder.".into()));
    }
    let mut arg = std::ffi::OsString::from("/D=");
    arg.push(dir.as_os_str());
    Ok(arg)
}

#[derive(Debug, Serialize)]
pub struct UpdateInfo {
    pub available: bool,
    pub current_version: String,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub date: Option<String>,
}

pub fn endpoint_for(repo: &str) -> AppResult<tauri::Url> {
    let repo = repo.trim().trim_matches('/');
    let ok = {
        let parts: Vec<&str> = repo.split('/').collect();
        parts.len() == 2
            && parts.iter().all(|p| !p.is_empty() && !p.starts_with('.') && p.len() <= 100 && p.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
    };
    if !ok {
        return Err(AppError::Update("update repository must look like owner/name".into()));
    }
    tauri::Url::parse(&format!("https://github.com/{repo}/releases/latest/download/latest.json")).map_err(|e| AppError::Update(e.to_string()))
}

fn updater<R: Runtime>(app: &AppHandle<R>, repo: &str, install_dir_arg: Option<std::ffi::OsString>) -> AppResult<tauri_plugin_updater::Updater> {
    let mut builder = app.updater_builder()
        .endpoints(vec![endpoint_for(repo)?])
        .map_err(|e| AppError::Update(e.to_string()))?
        .timeout(std::time::Duration::from_secs(30));
    if let Some(arg) = install_dir_arg {
        // The updater appends additional installer arguments after its own arguments.
        builder = builder.clear_installer_args().installer_arg(arg);
    }
    builder.build()
        .map_err(|e| AppError::Update(e.to_string()))
}

pub async fn check<R: Runtime>(app: &AppHandle<R>, repo: &str) -> AppResult<UpdateInfo> {
    ensure_stable()?;
    let current = app.package_info().version.to_string();
    let u = updater(app, repo, None)?.check().await.map_err(|e| AppError::Update(e.to_string()))?;
    Ok(match u {
        Some(u) => UpdateInfo { available: true, current_version: current, version: Some(u.version.clone()), notes: u.body.clone(), date: u.date.map(|d| d.to_string()) },
        None => UpdateInfo { available: false, current_version: current, version: None, notes: None, date: None },
    })
}

/// Download, verify the minisign signature (inside the plugin) and run the
/// installer. On Windows the installer closes the app and relaunches it.
pub async fn install<R: Runtime>(app: &AppHandle<R>, repo: &str, mut progress: impl FnMut(u64, Option<u64>)) -> AppResult<()> {
    ensure_stable()?;
    #[cfg(windows)]
    let install_dir_arg = Some(nsis_install_dir_arg(&std::env::current_exe().map_err(|e| AppError::Update(format!("Cannot locate the running Ycode executable: {e}")))?)?);
    #[cfg(not(windows))]
    let install_dir_arg = None;
    let u = updater(app, repo, install_dir_arg)?.check().await.map_err(|e| AppError::Update(e.to_string()))?.ok_or_else(|| AppError::Update("no update available".into()))?;
    #[cfg(windows)]
    if !u.download_url.path().to_ascii_lowercase().ends_with(".exe") {
        return Err(AppError::Update("This installation requires a signed NSIS .exe update package.".into()));
    }
    let mut done: u64 = 0;
    u.download_and_install(
        |chunk, total| {
            done += chunk as u64;
            progress(done, total);
        },
        || {},
    )
    .await
    .map_err(|e| AppError::Update(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_stable_builds_self_update() {
        assert!(updates_allowed("stable"));
        assert!(updates_allowed(""));
        for c in ["snapshot", "preview", "rc"] {
            assert!(!updates_allowed(c), "{c} builds must not self-update");
        }
        // Normal `cargo test` / CI builds are stable.
        assert_eq!(CHANNEL, "stable");
    }

    #[test]
    fn repo_validation() {
        assert_eq!(endpoint_for("acme/Ycode").unwrap().as_str(), "https://github.com/acme/Ycode/releases/latest/download/latest.json");
        assert!(endpoint_for("acme").is_err());
        assert!(endpoint_for("acme/ycode/extra").is_err());
        assert!(endpoint_for("ac me/y").is_err());
        assert!(endpoint_for("../x").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn nsis_update_targets_running_installation_with_spaces() {
        let dir = std::env::temp_dir().join(format!("Ycode update test {}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = dir.join("Ycode.exe");
        assert!(nsis_install_dir_arg(&exe).is_err(), "uninstalled copies must not be overwritten");
        std::fs::write(dir.join("uninstall.exe"), b"").unwrap();
        let arg = nsis_install_dir_arg(&exe).unwrap();
        let mut expected = std::ffi::OsString::from("/D=");
        expected.push(dir.as_os_str());
        assert_eq!(arg, expected);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
