//! GitHub Releases updater built on tauri-plugin-updater (signed artifacts only).
//! The repository is a user setting; the endpoint is the stable "latest" release
//! asset `latest.json`, which GitHub never resolves to a pre-release.

use crate::error::{AppError, AppResult};
use serde::Serialize;
use tauri::{AppHandle, Runtime};
use tauri_plugin_updater::UpdaterExt;

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

fn updater<R: Runtime>(app: &AppHandle<R>, repo: &str) -> AppResult<tauri_plugin_updater::Updater> {
    app.updater_builder()
        .endpoints(vec![endpoint_for(repo)?])
        .map_err(|e| AppError::Update(e.to_string()))?
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Update(e.to_string()))
}

pub async fn check<R: Runtime>(app: &AppHandle<R>, repo: &str) -> AppResult<UpdateInfo> {
    let current = app.package_info().version.to_string();
    let u = updater(app, repo)?.check().await.map_err(|e| AppError::Update(e.to_string()))?;
    Ok(match u {
        Some(u) => UpdateInfo { available: true, current_version: current, version: Some(u.version.clone()), notes: u.body.clone(), date: u.date.map(|d| d.to_string()) },
        None => UpdateInfo { available: false, current_version: current, version: None, notes: None, date: None },
    })
}

/// Download, verify the minisign signature (inside the plugin) and run the
/// installer. On Windows the installer closes the app and relaunches it.
pub async fn install<R: Runtime>(app: &AppHandle<R>, repo: &str, mut progress: impl FnMut(u64, Option<u64>)) -> AppResult<()> {
    let u = updater(app, repo)?.check().await.map_err(|e| AppError::Update(e.to_string()))?.ok_or_else(|| AppError::Update("no update available".into()))?;
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
    fn repo_validation() {
        assert_eq!(endpoint_for("acme/Ycode").unwrap().as_str(), "https://github.com/acme/Ycode/releases/latest/download/latest.json");
        assert!(endpoint_for("acme").is_err());
        assert!(endpoint_for("acme/ycode/extra").is_err());
        assert!(endpoint_for("ac me/y").is_err());
        assert!(endpoint_for("../x").is_err());
    }
}
