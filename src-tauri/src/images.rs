//! GPT Image generation and editing through OpenAI's Images API
//! (`POST /images/generations`, `POST /images/edits`).
//!
//! Exposed to every chat model as the local tools `generate_image` / `edit_image`.
//! Images are written to `<app data>/images/` and only their file name goes back
//! into the conversation, so history stays small and cache-friendly.

use crate::error::{AppError, AppResult, ProviderError, ProviderErrorKind};
use crate::providers::{Cancel, Endpoint};
use base64::Engine;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

pub const DEFAULT_MODEL: &str = "gpt-image-2.6-flare";
/// Models offered in Settings (any other ID can be typed by hand).
pub const KNOWN_MODELS: [&str; 4] = ["gpt-image-2.6-flare", "gpt-image-2.6-sunburst", "gpt-image-2.5-flare", "gpt-image-2.5-sunburst"];
pub const SIZES: [&str; 4] = ["auto", "1024x1024", "1536x1024", "1024x1536"];
pub const QUALITIES: [&str; 4] = ["auto", "low", "medium", "high"];
const MAX_SOURCE_BYTES: u64 = 25 * 1024 * 1024;
/// Docs: complex prompts may take up to 2 minutes.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Clone)]
pub struct ImageCtx {
    pub http: reqwest::Client,
    pub endpoint: Endpoint,
    pub dir: PathBuf,
    pub model: String,
}

pub enum ImageKind {
    Generate,
    Edit { source: PathBuf },
}

pub struct ImageJob {
    pub kind: ImageKind,
    pub prompt: String,
    pub size: String,
    pub quality: String,
    pub background: Option<String>,
    pub ctx: ImageCtx,
}

fn id_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^img-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.(png|jpeg|webp)$").expect("regex"))
}

pub fn is_image_id(s: &str) -> bool {
    id_re().is_match(s)
}

/// Path of a generated image, validated so an id can never escape the folder.
pub fn path_for(dir: &Path, id: &str) -> AppResult<PathBuf> {
    if !is_image_id(id) {
        return Err(AppError::Invalid(format!("'{id}' is not a Ycode image id")));
    }
    let p = dir.join(id);
    if !p.is_file() {
        return Err(AppError::NotFound(format!("image {id} no longer exists")));
    }
    Ok(p)
}

pub fn mime_for(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_string_lossy().to_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

pub fn generation_body(model: &str, prompt: &str, size: &str, quality: &str, background: Option<&str>) -> Value {
    let mut b = json!({ "model": model, "prompt": prompt, "n": 1, "size": size, "quality": quality, "output_format": "png" });
    if let Some(bg) = background {
        b["background"] = json!(bg);
    }
    b
}

fn classify(status: u16, body: &str) -> AppError {
    let mut e = ProviderError::from_status(status, body);
    if body.contains("moderation_blocked") {
        e.kind = ProviderErrorKind::Refused;
        e.retryable = false;
    }
    e.into()
}

/// Decode the first image of an Images API response.
pub fn decode_response(v: &Value) -> AppResult<(Vec<u8>, String, String)> {
    let b64 = v
        .pointer("/data/0/b64_json")
        .and_then(|s| s.as_str())
        .ok_or_else(|| AppError::Provider(ProviderError::new(ProviderErrorKind::Unknown, "image response contained no image data")))?;
    let bytes = base64::engine::general_purpose::STANDARD.decode(b64.trim()).map_err(|e| AppError::Invalid(format!("invalid image data: {e}")))?;
    let fmt = v.get("output_format").and_then(|s| s.as_str()).unwrap_or("png").to_string();
    let u = &v["usage"];
    let usage = match (u.get("input_tokens").and_then(|x| x.as_u64()), u.get("output_tokens").and_then(|x| x.as_u64())) {
        (Some(i), Some(o)) => format!("input {i} tokens, output {o} tokens"),
        _ => "usage not reported".into(),
    };
    Ok((bytes, fmt, usage))
}

pub async fn run(job: ImageJob, cancel: &Cancel) -> AppResult<(String, Vec<String>)> {
    let ImageJob { kind, prompt, size, quality, background, ctx } = job;
    if ctx.endpoint.api_key.is_empty() {
        return Err(ProviderError::new(ProviderErrorKind::MissingKey, "image generation needs an OpenAI API key").into());
    }
    let req = match &kind {
        ImageKind::Generate => ctx
            .http
            .post(ctx.endpoint.url("/images/generations"))
            .bearer_auth(&ctx.endpoint.api_key)
            .json(&generation_body(&ctx.model, &prompt, &size, &quality, background.as_deref())),
        ImageKind::Edit { source } => {
            let bytes = tokio::fs::read(source).await?;
            let name = source.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "image.png".into());
            let mime = mime_for(source).unwrap_or("image/png");
            let part = reqwest::multipart::Part::bytes(bytes).file_name(name).mime_str(mime).map_err(|e| AppError::Invalid(e.to_string()))?;
            let mut form = reqwest::multipart::Form::new()
                .text("model", ctx.model.clone())
                .text("prompt", prompt.clone())
                .text("size", size.clone())
                .text("quality", quality.clone())
                .text("n", "1")
                .part("image[]", part);
            if let Some(bg) = &background {
                form = form.text("background", bg.clone());
            }
            ctx.http.post(ctx.endpoint.url("/images/edits")).bearer_auth(&ctx.endpoint.api_key).multipart(form)
        }
    };
    let send = req.timeout(REQUEST_TIMEOUT).send();
    let resp = tokio::select! {
        r = send => r.map_err(|e| AppError::Provider(ProviderError::new(if e.is_timeout() { ProviderErrorKind::Timeout } else { ProviderErrorKind::Network }, e.without_url().to_string())))?,
        _ = cancel.cancelled() => return Err(AppError::Cancelled),
    };
    let status = resp.status().as_u16();
    let body = tokio::select! {
        b = resp.text() => b.map_err(|e| AppError::Provider(ProviderError::new(ProviderErrorKind::StreamInterrupted, e.to_string())))?,
        _ = cancel.cancelled() => return Err(AppError::Cancelled),
    };
    if status >= 400 {
        return Err(classify(status, &body));
    }
    let v: Value = serde_json::from_str(&body).map_err(|e| AppError::Provider(ProviderError::new(ProviderErrorKind::Unknown, e.to_string())))?;
    let (bytes, fmt, usage) = decode_response(&v)?;
    let ext = match fmt.as_str() {
        "jpeg" | "jpg" => "jpeg",
        "webp" => "webp",
        _ => "png",
    };
    tokio::fs::create_dir_all(&ctx.dir).await?;
    let id = format!("img-{}.{ext}", uuid::Uuid::new_v4());
    tokio::fs::write(ctx.dir.join(&id), &bytes).await?;
    let verb = if matches!(kind, ImageKind::Edit { .. }) { "Edited" } else { "Generated" };
    let text = format!(
        "{verb} 1 image with {model}: {id} (size {size}, quality {quality}; {usage}). It is already displayed to the user. You have not visually inspected the image; do not claim it matches the request or describe unverified details. To change it, call edit_image with image=\"{id}\".",
        model = ctx.model
    );
    Ok((text, vec![id]))
}

/// `data:` URL for showing a generated image in the UI.
pub fn data_url(dir: &Path, id: &str) -> AppResult<String> {
    let p = path_for(dir, id)?;
    let bytes = std::fs::read(&p)?;
    Ok(format!("data:{};base64,{}", mime_for(&p).unwrap_or("image/png"), base64::engine::general_purpose::STANDARD.encode(bytes)))
}

pub fn check_source_size(p: &Path) -> AppResult<()> {
    let len = std::fs::metadata(p).map_err(|e| AppError::NotFound(format!("{}: {e}", p.display())))?.len();
    if len > MAX_SOURCE_BYTES {
        return Err(AppError::Invalid("source image is larger than 25 MB".into()));
    }
    if mime_for(p).is_none() {
        return Err(AppError::Invalid("source image must be PNG, JPEG or WebP".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_strict() {
        assert!(is_image_id("img-0f8e2c1a-1b2c-4d5e-8f90-123456789abc.png"));
        assert!(!is_image_id("../img-0f8e2c1a-1b2c-4d5e-8f90-123456789abc.png"));
        assert!(!is_image_id("img-x.png"));
        assert!(!is_image_id("img-0f8e2c1a-1b2c-4d5e-8f90-123456789abc.exe"));
    }

    #[test]
    fn builds_generation_body() {
        let b = generation_body("gpt-image-2.6-flare", "a cat", "1024x1024", "high", Some("transparent"));
        assert_eq!(b["model"], "gpt-image-2.6-flare");
        assert_eq!(b["n"], 1);
        assert_eq!(b["output_format"], "png");
        assert_eq!(b["background"], "transparent");
        assert!(generation_body("m", "p", "auto", "auto", None).get("background").is_none());
    }

    #[test]
    fn decodes_response_and_usage() {
        let png = [0x89u8, b'P', b'N', b'G'];
        let v = json!({"created":1,"data":[{"b64_json": base64::engine::general_purpose::STANDARD.encode(png)}],"output_format":"png","usage":{"input_tokens":12,"output_tokens":4160,"total_tokens":4172}});
        let (bytes, fmt, usage) = decode_response(&v).unwrap();
        assert_eq!(bytes, png);
        assert_eq!(fmt, "png");
        assert!(usage.contains("4160"));
        assert!(decode_response(&json!({"data":[]})).is_err());
    }

    #[test]
    fn moderation_block_is_a_refusal() {
        let e = classify(400, r#"{"error":{"code":"moderation_blocked","message":"Your request was rejected by the safety system."}}"#);
        match e {
            AppError::Provider(p) => assert_eq!(p.kind, ProviderErrorKind::Refused),
            other => panic!("{other:?}"),
        }
    }
}
