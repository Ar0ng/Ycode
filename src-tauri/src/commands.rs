//! Tauri IPC commands. The UI never receives API keys; it only learns whether one
//! is stored.

use crate::agent::{self, AgentEvent, RunParams};
use crate::error::{AppError, AppResult};
use crate::model::{Attachment, AttachmentKind, ProviderId};
use crate::providers::{self, registry::ModelCaps};
use crate::secrets;
use crate::skills::SkillMeta;
use crate::state::{AppState, Decision, lock, validate_base_url};
use crate::storage::{Conversation, ConversationSummary, Settings};
use serde::Serialize;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;
use tauri::State;
use tauri::ipc::Channel;

type St<'a> = State<'a, Arc<AppState>>;

fn provider(s: &str) -> AppResult<ProviderId> {
    ProviderId::parse(s).ok_or_else(|| AppError::Invalid(format!("unknown provider '{s}'")))
}

#[derive(Serialize)]
pub struct ProviderInfo {
    id: ProviderId,
    has_key: bool,
    base_url: String,
    default_base_url: &'static str,
    refreshed_at: Option<i64>,
}

#[derive(Serialize)]
pub struct Bootstrap {
    version: String,
    settings: Settings,
    providers: Vec<ProviderInfo>,
    models: Vec<ModelCaps>,
    conversations: Vec<ConversationSummary>,
    skills: Vec<SkillMeta>,
    recovery_notice: Option<String>,
    schema_version: i64,
}

fn providers_info(st: &AppState, s: &Settings) -> Vec<ProviderInfo> {
    ProviderId::ALL
        .iter()
        .map(|p| ProviderInfo {
            id: *p,
            has_key: st.has_key(*p),
            base_url: s.base_urls.get(p.as_str()).cloned().unwrap_or_default(),
            default_base_url: p.default_base_url(),
            refreshed_at: s.remote_refreshed_at.get(p.as_str()).copied(),
        })
        .collect()
}

#[tauri::command]
pub fn bootstrap(app: tauri::AppHandle, st: St) -> AppResult<Bootstrap> {
    let s = st.settings();
    Ok(Bootstrap {
        version: app.package_info().version.to_string(),
        providers: providers_info(&st, &s),
        models: st.model_table(&s),
        conversations: st.db.list_conversations()?,
        skills: st.skills_snapshot().list().to_vec(),
        recovery_notice: lock(&st.db.recovery_notice).take(),
        schema_version: st.db.schema_version(),
        settings: s,
    })
}

const PATCHABLE: [&str; 11] =
    ["language", "theme", "provider", "mode", "models", "efforts", "update_repo", "check_updates_on_start", "claude_fallbacks", "sidebar_collapsed", "max_steps"];

#[tauri::command]
pub fn update_settings(st: St, patch: Value) -> AppResult<Settings> {
    let Value::Object(patch) = patch else { return Err(AppError::Invalid("patch must be an object".into())) };
    st.update_settings(|s| {
        let mut cur = serde_json::to_value(&*s).unwrap_or(Value::Null);
        for (k, v) in patch {
            if PATCHABLE.contains(&k.as_str()) {
                cur[k] = v;
            }
        }
        if let Ok(next) = serde_json::from_value::<Settings>(cur) {
            *s = next;
        }
    })
}

#[tauri::command]
pub fn set_api_key(st: St, provider_id: String, key: String) -> AppResult<Vec<ProviderInfo>> {
    secrets::set_key(provider(&provider_id)?, &key)?;
    crate::logging::info(&format!("api key stored for {provider_id}"));
    Ok(providers_info(&st, &st.settings()))
}

#[tauri::command]
pub fn delete_api_key(st: St, provider_id: String) -> AppResult<Vec<ProviderInfo>> {
    secrets::delete_key(provider(&provider_id)?)?;
    crate::logging::info(&format!("api key deleted for {provider_id}"));
    Ok(providers_info(&st, &st.settings()))
}

#[tauri::command]
pub fn set_base_url(st: St, provider_id: String, url: String) -> AppResult<Vec<ProviderInfo>> {
    let p = provider(&provider_id)?;
    let url = validate_base_url(&url)?;
    let s = st.update_settings(|s| {
        if url.is_empty() {
            s.base_urls.remove(p.as_str());
        } else {
            s.base_urls.insert(p.as_str().into(), url);
        }
    })?;
    Ok(providers_info(&st, &s))
}

#[derive(Serialize)]
pub struct RefreshResult {
    models: Vec<ModelCaps>,
    returned: usize,
}

/// Calls the vendor's list-models endpoint (also serves as the connection test:
/// it validates key + base URL without spending tokens).
#[tauri::command]
pub async fn refresh_models(st: St<'_>, provider_id: String) -> AppResult<RefreshResult> {
    let p = provider(&provider_id)?;
    let ep = st.endpoint(p)?;
    let remote = providers::list_models(&st.http, &ep, p).await?;
    let returned = remote.len();
    let s = st.update_settings(|s| {
        s.remote_models.insert(p.as_str().into(), remote);
        s.remote_refreshed_at.insert(p.as_str().into(), crate::model::now_ms());
    })?;
    Ok(RefreshResult { models: st.model_table(&s), returned })
}

#[tauri::command]
pub fn add_manual_model(st: St, provider_id: String, model_id: String) -> AppResult<Vec<ModelCaps>> {
    let p = provider(&provider_id)?;
    let id = model_id.trim().to_string();
    if id.is_empty() || id.len() > 128 || id.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(AppError::Invalid("invalid model ID".into()));
    }
    let s = st.update_settings(|s| {
        let v = s.manual_models.entry(p.as_str().into()).or_default();
        if !v.contains(&id) {
            v.push(id.clone());
        }
        s.models.insert(p.as_str().into(), id.clone());
    })?;
    Ok(st.model_table(&s))
}

#[tauri::command]
pub fn remove_manual_model(st: St, provider_id: String, model_id: String) -> AppResult<Vec<ModelCaps>> {
    let p = provider(&provider_id)?;
    let s = st.update_settings(|s| {
        if let Some(v) = s.manual_models.get_mut(p.as_str()) {
            v.retain(|m| m != &model_id);
        }
    })?;
    Ok(st.model_table(&s))
}

#[tauri::command]
pub fn list_conversations(st: St) -> AppResult<Vec<ConversationSummary>> {
    st.db.list_conversations()
}

#[tauri::command]
pub fn get_conversation(st: St, id: String) -> AppResult<Conversation> {
    st.db.get_conversation(&id)
}

#[tauri::command]
pub fn delete_conversation(st: St, id: String) -> AppResult<()> {
    if let Some(c) = lock(&st.runs).get(&id) {
        c.cancel();
    }
    lock(&st.perms).remove(&id);
    st.db.delete_conversation(&id)
}

#[tauri::command]
pub fn send_message(st: St, params: RunParams, on_event: Channel<AgentEvent>) -> AppResult<()> {
    if let Some(t) = &params.text {
        if t.trim().is_empty() && params.attachments.is_empty() {
            return Err(AppError::Invalid("empty message".into()));
        }
    }
    let cancel = st.register_run(&params.conversation_id)?;
    let state = st.inner().clone();
    let emit: agent::Emit = Arc::new(move |e| {
        let _ = on_event.send(e);
    });
    tauri::async_runtime::spawn(agent::run(state, params, cancel, emit));
    Ok(())
}

#[tauri::command]
pub fn stop_run(st: St, conversation_id: String) {
    if let Some(c) = lock(&st.runs).get(&conversation_id) {
        c.cancel();
    }
}

#[tauri::command]
pub fn respond_approval(st: St, request_id: String, decision: Decision) -> AppResult<()> {
    agent::respond_approval(&st, &request_id, decision)
}

#[tauri::command]
pub fn set_workspace(st: St, path: Option<String>) -> AppResult<Settings> {
    let resolved = match path {
        None => None,
        Some(p) => {
            let real = crate::tools::sandbox::dunce_canonicalize(Path::new(&p))?;
            if !real.is_dir() {
                return Err(AppError::Invalid("not a folder".into()));
            }
            if real.parent().is_none() {
                return Err(AppError::Permission("choose a project folder, not a whole drive".into()));
            }
            let lower = real.to_string_lossy().to_lowercase();
            if let Ok(win) = std::env::var("SystemRoot") {
                if lower.starts_with(&win.to_lowercase()) {
                    return Err(AppError::Permission("system folders can't be used as a workspace".into()));
                }
            }
            Some(real.to_string_lossy().to_string())
        }
    };
    st.update_settings(|s| {
        s.workspace = resolved.clone();
        if let Some(r) = resolved {
            s.recent_workspaces.retain(|x| x != &r);
            s.recent_workspaces.insert(0, r);
            s.recent_workspaces.truncate(8);
        }
    })
}

const TEXT_MAX: u64 = 1024 * 1024;

pub fn classify_attachment(path: &Path) -> AppResult<Attachment> {
    let meta = std::fs::metadata(path).map_err(|e| AppError::NotFound(format!("{}: {e}", path.display())))?;
    if !meta.is_file() {
        return Err(AppError::Invalid("not a file".into()));
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let (kind, mime, max) = match ext.as_str() {
        "txt" | "log" | "csv" => (AttachmentKind::Text, "text/plain", TEXT_MAX),
        "md" | "markdown" => (AttachmentKind::Text, "text/markdown", TEXT_MAX),
        "png" => (AttachmentKind::Image, "image/png", 20 << 20),
        "jpg" | "jpeg" => (AttachmentKind::Image, "image/jpeg", 20 << 20),
        "webp" => (AttachmentKind::Image, "image/webp", 20 << 20),
        "pdf" => (AttachmentKind::Pdf, "application/pdf", 32 << 20),
        "docx" => (AttachmentKind::Office, "application/vnd.openxmlformats-officedocument.wordprocessingml.document", 50 << 20),
        "xlsx" => (AttachmentKind::Office, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", 50 << 20),
        "pptx" => (AttachmentKind::Office, "application/vnd.openxmlformats-officedocument.presentationml.presentation", 50 << 20),
        _ => return Err(AppError::Invalid(format!("unsupported file type .{ext}"))),
    };
    if meta.len() > max {
        return Err(AppError::Invalid(format!("{name} is larger than {} MB", max >> 20)));
    }
    let text = if kind == AttachmentKind::Text {
        let bytes = std::fs::read(path)?;
        if bytes.iter().take(8192).any(|b| *b == 0) {
            return Err(AppError::Invalid(format!("{name} is not a text file")));
        }
        let s = String::from_utf8_lossy(&bytes).into_owned();
        Some(s.strip_prefix('\u{feff}').map(String::from).unwrap_or(s))
    } else {
        None
    };
    Ok(Attachment { id: uuid::Uuid::new_v4().to_string(), name, mime: mime.into(), size: meta.len(), kind, text })
}

#[tauri::command]
pub fn inspect_attachment(path: String) -> AppResult<Attachment> {
    classify_attachment(Path::new(&path))
}

// ---------- skills ----------

fn persist_disabled(st: &AppState) -> AppResult<Vec<SkillMeta>> {
    let list = st.skills_snapshot().list().to_vec();
    let disabled: Vec<String> = list.iter().filter(|s| !s.enabled).map(|s| s.name.clone()).collect();
    st.update_settings(|s| s.disabled_skills = disabled)?;
    Ok(list)
}

#[tauri::command]
pub fn list_skills(st: St) -> Vec<SkillMeta> {
    st.skills_snapshot().list().to_vec()
}

#[tauri::command]
pub fn install_skill(st: St, path: String, overwrite: bool) -> AppResult<Vec<SkillMeta>> {
    st.skills.write().unwrap_or_else(|p| p.into_inner()).install_from(Path::new(&path), overwrite)?;
    persist_disabled(&st)
}

#[tauri::command]
pub fn set_skill_enabled(st: St, name: String, enabled: bool) -> AppResult<Vec<SkillMeta>> {
    st.skills.write().unwrap_or_else(|p| p.into_inner()).set_enabled(&name, enabled);
    persist_disabled(&st)
}

#[tauri::command]
pub fn delete_skill(st: St, name: String) -> AppResult<Vec<SkillMeta>> {
    st.skills.write().unwrap_or_else(|p| p.into_inner()).remove(&name)?;
    persist_disabled(&st)
}

#[tauri::command]
pub fn validate_skill(path: String) -> AppResult<SkillMeta> {
    crate::skills::validate_dir(Path::new(&path))
}

// ---------- updates ----------

#[tauri::command]
pub async fn check_update(app: tauri::AppHandle, st: St<'_>) -> AppResult<crate::updater::UpdateInfo> {
    let repo = st.settings().update_repo;
    if repo.trim().is_empty() {
        return Err(AppError::Update("update repository is not configured".into()));
    }
    crate::updater::check(&app, &repo).await
}

#[derive(Clone, Serialize)]
pub struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

#[tauri::command]
pub async fn install_update(app: tauri::AppHandle, st: St<'_>, on_progress: Channel<Progress>) -> AppResult<()> {
    let repo = st.settings().update_repo;
    crate::updater::install(&app, &repo, |downloaded, total| {
        let _ = on_progress.send(Progress { downloaded, total });
    })
    .await
}

#[tauri::command]
pub fn open_data_folder(st: St) -> AppResult<String> {
    Ok(st.data_dir.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attachment_classification() {
        let d = std::env::temp_dir().join(format!("ycode-att-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a.md"), "\u{feff}# hi").unwrap();
        std::fs::write(d.join("b.png"), [0x89, b'P', b'N', b'G']).unwrap();
        std::fs::write(d.join("c.exe"), "MZ").unwrap();
        let a = classify_attachment(&d.join("a.md")).unwrap();
        assert_eq!(a.kind, AttachmentKind::Text);
        assert_eq!(a.text.as_deref(), Some("# hi"));
        let b = classify_attachment(&d.join("b.png")).unwrap();
        assert_eq!(b.kind, AttachmentKind::Image);
        assert!(b.text.is_none() && !b.kind.supported_in_v1());
        assert!(classify_attachment(&d.join("c.exe")).is_err());
    }
}
