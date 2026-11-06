//! Local browser screenshots followed by a separate DeepSeek Flash vision request.
use crate::error::{AppError, AppResult, ProviderError, ProviderErrorKind};
use crate::providers::{Cancel, Endpoint};
use base64::Engine;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

fn browser() -> AppResult<PathBuf> {
    let candidates = [
        r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Microsoft\Edge\Application\msedge.exe",
        r"C:\Program Files\Google\Chrome\Application\chrome.exe",
    ];
    candidates.into_iter().map(PathBuf::from).find(|p| p.is_file())
        .ok_or_else(|| AppError::NotFound("Edge or Chrome is required for visual inspection".into()))
}

fn vision_body(question: &str, frames: &[Vec<u8>]) -> Value {
    let mut content = vec![json!({"type":"text", "text":format!("These are ordered frames at 0.5, 1.1 and 1.7 seconds of a local visual artifact. Review the user's requirements, composition, anatomy, overlap, cropping, typography and animation continuity. State uncertainty and give actionable corrections. Requirements: {question}")})];
    for bytes in frames {
        let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
        content.push(json!({"type":"image_url", "image_url":{"url":format!("data:image/png;base64,{b64}")}}));
    }
    json!({"model":"deepseek-flash", "thinking":{"type":"disabled"}, "messages":[{"role":"user", "content":content}], "max_tokens":2048})
}

struct Profile { path: PathBuf, root: PathBuf }
impl Drop for Profile {
    fn drop(&mut self) {
        // Only remove the owned, non-symlink directory directly under the verified root.
        let safe = std::fs::symlink_metadata(&self.path).ok().is_some_and(|m| m.is_dir() && !m.file_type().is_symlink())
            && std::fs::canonicalize(&self.path).ok().and_then(|p| p.parent().map(|p| p.to_path_buf())) == std::fs::canonicalize(&self.root).ok();
        if safe { let _ = std::fs::remove_dir_all(&self.path); }
    }
}

pub async fn inspect(path: PathBuf, question: String, http: reqwest::Client, endpoint: Endpoint, dir: PathBuf, cancel: &Cancel) -> AppResult<(String, Vec<String>)> {
    if endpoint.api_key.is_empty() { return Err(AppError::Invalid("DeepSeek API key is required for visual inspection".into())); }
    let browser = browser()?;
    let url = reqwest::Url::from_file_path(&path).map_err(|_| AppError::Invalid("cannot convert the visual file path to a URL".into()))?;
    tokio::fs::create_dir_all(&dir).await?;
    let dir = super::sandbox::dunce_canonicalize(&dir)?;
    let profile_path = dir.join(format!("visual-profile-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&profile_path)?;
    let profile = Profile { path: profile_path, root: dir.clone() };
    let mut frames = Vec::new();
    let mut ids = Vec::new();
    for ms in [500, 1100, 1700] {
        let id = format!("img-{}.png", uuid::Uuid::new_v4());
        let dest = dir.join(&id);
        let mut command = tokio::process::Command::new(&browser);
        command.arg("--headless=new").arg("--disable-gpu").arg("--no-first-run").arg("--no-default-browser-check")
            .arg(format!("--user-data-dir={}", profile.path.display()))
            .arg("--window-size=1200,800").arg("--hide-scrollbars")
            .arg(format!("--virtual-time-budget={ms}"))
            .arg(format!("--screenshot={}", dest.display())).arg(url.as_str())
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(0x0800_0000);
        let mut child = command.spawn().map_err(|e| AppError::Io(format!("cannot launch browser: {e}")))?;
        let status = tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(30), child.wait()) => result.map_err(|_| AppError::Invalid("visual screenshot timed out".into()))??,
            _ = cancel.cancelled() => return Err(AppError::Cancelled),
        };
        if !status.success() { return Err(AppError::Invalid("browser failed to capture visual frame".into())); }
        let bytes = tokio::fs::read(&dest).await?;
        if bytes.len() > 8 * 1024 * 1024 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") { return Err(AppError::Invalid("invalid or oversized screenshot".into())); }
        frames.push(bytes);
        ids.push(id);
    }
    let response = tokio::select! {
        r = http.post(endpoint.url("/chat/completions")).bearer_auth(&endpoint.api_key).json(&vision_body(&question, &frames)).timeout(Duration::from_secs(120)).send() => r.map_err(|e| AppError::Provider(ProviderError::new(ProviderErrorKind::Network, e.without_url().to_string())))?,
        _ = cancel.cancelled() => return Err(AppError::Cancelled),
    };
    let status = response.status().as_u16();
    let body = tokio::select! {
        r = response.text() => r.map_err(|e| AppError::Provider(ProviderError::new(ProviderErrorKind::Network, e.without_url().to_string())))?,
        _ = cancel.cancelled() => return Err(AppError::Cancelled),
    };
    if status >= 400 {
        return Ok((format!("Screenshots captured, but DeepSeek Flash inspection failed (HTTP {status}); do not claim a visual review. Additional request may have been billed."), ids));
    }
    let v: Value = serde_json::from_str(&body).map_err(|e| AppError::Invalid(format!("invalid vision response: {e}")))?;
    let text = v.pointer("/choices/0/message/content").and_then(Value::as_str).filter(|s| !s.is_empty()).ok_or_else(|| AppError::Invalid("vision response contained no critique".into()))?;
    let usage = &v["usage"];
    Ok((format!("Visual critique by DeepSeek Flash (three screenshots, 0.5/1.1/1.7 s):\n{}\n\nSeparate vision usage: input {} tokens, output {} tokens (not included in conversation totals).", crate::error::truncate(text, 12000), usage["prompt_tokens"].as_u64().map_or("unknown".into(), |n| n.to_string()), usage["completion_tokens"].as_u64().map_or("unknown".into(), |n| n.to_string())), ids))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn vision_request_contains_ordered_images() {
        let b = vision_body("check pose", &[vec![1, 2], vec![3]]);
        assert_eq!(b["model"], "deepseek-flash");
        assert_eq!(b["messages"][0]["content"].as_array().unwrap().len(), 3);
        assert_eq!(b["messages"][0]["content"][1]["image_url"]["url"], "data:image/png;base64,AQI=");
    }
}
