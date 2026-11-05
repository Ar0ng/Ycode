//! Agent runtime: one user message → loop of (model turn → tool calls → results)
//! until the model stops calling tools, the step budget runs out or the user stops.
//! History is append-only so provider prefix caches stay valid.

pub mod prompts;

use crate::error::{AppError, AppResult, ProviderError, ProviderErrorKind};
use crate::model::*;
use crate::providers::registry::{EffortUpdate, ModelCaps, clamp_effort};
use crate::providers::{self, Cancel, StopKind, StreamEvent, TurnRequest, TurnResult};
use crate::state::{AppState, Decision, lock};
use crate::tools::{self, ApprovalKind, ApprovalRequest, ToolCtx};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::oneshot;

pub type Emit = Arc<dyn Fn(AgentEvent) + Send + Sync>;

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// A message was persisted/created (user, assistant placeholder, tool results, control).
    Message { message: Message },
    Delta { message_id: String, kind: &'static str, text: String },
    ToolStatus {
        message_id: String,
        call_id: String,
        name: String,
        /// pending | running | success | failed | cancelled
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        approval: Option<ApprovalPrompt>,
    },
    Usage { turn: Usage, total: Usage },
    Notice { code: String, detail: String },
    Error { message_id: Option<String>, error: AppError },
    Title { conversation_id: String, title: String },
    Finished { stopped: bool },
}

#[derive(Debug, Clone, Serialize)]
pub struct ApprovalPrompt {
    pub request_id: String,
    #[serde(flatten)]
    pub request: ApprovalRequest,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunParams {
    pub conversation_id: String,
    /// None = retry/continue from the existing history.
    pub text: Option<String>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    pub provider: ProviderId,
    pub model: String,
    pub effort: Option<Effort>,
    pub mode: Mode,
    /// Sidebar group for a brand-new conversation.
    #[serde(default)]
    pub group_id: Option<String>,
}

fn title_from(text: &str) -> String {
    let first = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("New chat");
    let clean: String = first.trim_start_matches(['#', '>', '-', '*', ' ']).chars().filter(|c| !c.is_control()).collect();
    crate::error::truncate(clean.trim(), 40)
}

/// Decide the request-level effort and whether an in-band change record is needed.
/// Returns (request_effort, control_to_append).
pub fn plan_effort(history: &[Message], provider: ProviderId, caps: &ModelCaps, wanted: Option<Effort>) -> (Option<Effort>, Option<Control>) {
    let wanted = clamp_effort(caps, wanted);
    let Some(wanted) = wanted else { return (None, None) };
    let mut base: Option<Effort> = None;
    let mut effective: Option<Effort> = None;
    for m in history {
        match (&m.role, &m.control) {
            (Role::Control, Some(Control::Effort { provider: p, model, effort, in_band })) if *p == provider && *model == caps.id => {
                if !*in_band {
                    base = Some(*effort);
                }
                effective = Some(*effort);
            }
            (Role::Assistant, _) if m.provider == Some(provider) && m.model.as_deref() == Some(caps.id.as_str()) && m.status != "error" => {
                if base.is_none() {
                    base = m.effort;
                }
                if m.effort.is_some() {
                    effective = m.effort;
                }
            }
            _ => {}
        }
    }
    match (effective, base) {
        (Some(eff), Some(b)) if eff != wanted => {
            let in_band = caps.effort_update != EffortUpdate::Param;
            let ctl = Control::Effort { provider, model: caps.id.clone(), effort: wanted, in_band };
            (Some(if in_band { b } else { wanted }), Some(ctl))
        }
        (Some(_), Some(b)) => (Some(if caps.effort_update != EffortUpdate::Param { b } else { wanted }), None),
        _ => (Some(wanted), None),
    }
}

fn request_history(all: &[Message]) -> Vec<Message> {
    all.iter()
        .filter(|m| !(m.role == Role::Assistant && m.status == "error"))
        .filter(|m| !(m.role == Role::Assistant && m.text.is_empty() && m.tool_calls.is_empty() && m.raw.is_none()))
        .cloned()
        .collect()
}

fn estimate_tokens(history: &[Message], system: &[String]) -> u64 {
    let mut chars: usize = system.iter().map(|s| s.len()).sum();
    for m in history {
        chars += m.user_content_text().len() + m.tool_results.iter().map(|r| r.content.len()).sum::<usize>();
        chars += m.raw.as_ref().map(|r| r.data.to_string().len()).unwrap_or(0);
        chars += m.tool_calls.iter().map(|c| c.arguments.to_string().len()).sum::<usize>();
    }
    (chars / 3) as u64
}

/// When the context is close to full, replace the oldest tool outputs with a
/// placeholder. This breaks the cached prefix once, so it only runs when needed;
/// the latest 12 messages are never touched. Returns how many results were trimmed.
pub fn trim_context(history: &mut [Message], budget: u64, system: &[String]) -> usize {
    let mut trimmed = 0;
    let n = history.len();
    let mut i = 0;
    while estimate_tokens(history, system) > budget && i + 12 < n {
        if history[i].role == Role::Tool {
            for r in history[i].tool_results.iter_mut() {
                if r.content.len() > 200 && !r.content.starts_with("[older tool output removed") {
                    r.content = format!("[older tool output removed to fit the context window; call {} again if needed]", r.name);
                    trimmed += 1;
                }
            }
        }
        i += 1;
    }
    trimmed
}

pub async fn run(state: Arc<AppState>, p: RunParams, cancel: Cancel, emit: Emit) {
    let conv = p.conversation_id.clone();
    let result = run_inner(&state, p, &cancel, &emit).await;
    let stopped = match result {
        Ok(stopped) => stopped,
        Err(AppError::Cancelled) => true,
        Err(e) => {
            emit(AgentEvent::Error { message_id: None, error: e });
            false
        }
    };
    state.finish_run(&conv);
    emit(AgentEvent::Finished { stopped });
}

async fn run_inner(state: &Arc<AppState>, p: RunParams, cancel: &Cancel, emit: &Emit) -> AppResult<bool> {
    let settings = state.settings();
    let skills = state.skills_snapshot();
    let workspace: Option<String> = settings.workspace.clone();
    let ws_path: Option<PathBuf> = workspace.as_ref().map(PathBuf::from).filter(|p| p.is_dir());
    let caps = state.caps_for(&settings, p.provider, &p.model);

    // Load or create the conversation.
    let mut messages = match state.db.get_conversation(&p.conversation_id) {
        Ok(c) => c.messages,
        Err(AppError::NotFound(_)) => {
            state.db.create_conversation_in(&p.conversation_id, p.mode, workspace.as_deref(), p.group_id.as_deref())?;
            vec![]
        }
        Err(e) => return Err(e),
    };
    state.db.set_conversation_meta(&p.conversation_id, p.mode, workspace.as_deref())?;

    // Effort planning (may append an in-band change record before the new user turn).
    let (request_effort, control) = plan_effort(&messages, p.provider, &caps, p.effort);
    let effective_effort = clamp_effort(&caps, p.effort);
    if let Some(e) = effective_effort {
        let key = format!("{}:{}", p.provider.as_str(), caps.id);
        let _ = state.update_settings(|s| {
            s.efforts.insert(key, e);
            s.provider = p.provider;
            s.models.insert(p.provider.as_str().into(), caps.id.clone());
            s.mode = p.mode;
        });
    }
    if let Some(ctl) = control {
        let mut m = Message::new(Role::Control);
        m.control = Some(ctl);
        state.db.upsert_message(&p.conversation_id, &m)?;
        emit(AgentEvent::Message { message: m.clone() });
        messages.push(m);
    }

    if let Some(text) = &p.text {
        if let Some(bad) = p.attachments.iter().find(|a| !a.kind.supported_in_v1()) {
            return Err(AppError::Invalid(format!("attachment '{}' is not supported yet", bad.name)));
        }
        let mut u = Message::new(Role::User);
        u.text = text.clone();
        u.attachments = p.attachments.clone();
        for name in skills.mentioned(text) {
            let body = skills.load_instructions(&name)?;
            u.attachments.push(Attachment {
                id: uuid::Uuid::new_v4().to_string(),
                name: format!("skill:{name}"),
                mime: "text/markdown".into(),
                size: body.len() as u64,
                kind: AttachmentKind::Text,
                text: Some(body),
            });
        }
        u.mode = Some(p.mode);
        state.db.upsert_message(&p.conversation_id, &u)?;
        emit(AgentEvent::Message { message: u.clone() });
        let first_user = !messages.iter().any(|m| m.role == Role::User);
        messages.push(u);
        if first_user {
            let title = title_from(text);
            state.db.set_title(&p.conversation_id, &title)?;
            emit(AgentEvent::Title { conversation_id: p.conversation_id.clone(), title });
        }
    } else {
        // Retry: drop a trailing failed assistant message, require a user turn last.
        while let Some(last) = messages.last() {
            if last.role == Role::Assistant && last.status == "error" {
                state.db.delete_message(&p.conversation_id, &last.id)?;
                messages.pop();
            } else {
                break;
            }
        }
        if !matches!(messages.last().map(|m| m.role), Some(Role::User) | Some(Role::Tool) | Some(Role::Control)) {
            return Err(AppError::Invalid("nothing to retry".into()));
        }
    }

    // GPT Image tools: available from any chat model when enabled and an OpenAI key exists.
    let image_ctx = if settings.image_generation && state.has_key(ProviderId::Openai) {
        Some(crate::images::ImageCtx {
            http: state.http.clone(),
            endpoint: state.endpoint(ProviderId::Openai)?,
            dir: state.data_dir.join("images"),
            model: if settings.image_model.trim().is_empty() { crate::images::DEFAULT_MODEL.to_string() } else { settings.image_model.trim().to_string() },
        })
    } else {
        None
    };
    let tool_specs = if caps.tools { tools::specs_with(p.mode, image_ctx.is_some()) } else { vec![] };
    let system = prompts::system_blocks(p.mode, &skills, ws_path.as_ref().map(|p| p.to_string_lossy().to_string()).as_deref());
    let endpoint = state.endpoint(p.provider)?;
    let max_output = caps.max_output.clamp(1024, 64_000);
    let cache_key = format!("ycode-{:016x}", fnv64(p.conversation_id.as_bytes()));
    let max_steps = settings.max_steps.clamp(1, 200);

    for _step in 0..max_steps {
        if cancel.is_cancelled() {
            return Ok(true);
        }
        let budget = caps.context_window.saturating_sub(max_output).saturating_mul(85) / 100;
        let trimmed = trim_context(&mut messages, budget, &system);
        if trimmed > 0 {
            emit(AgentEvent::Notice { code: "context_trimmed".into(), detail: trimmed.to_string() });
        }
        let history = request_history(&messages);
        let req = TurnRequest {
            provider: p.provider,
            model: &caps.id,
            caps: &caps,
            system: &system,
            tools: &tool_specs,
            history: &history,
            effort: request_effort,
            cache_key: &cache_key,
            max_output,
            fallbacks: settings.claude_fallbacks,
        };

        let mut a = Message::new(Role::Assistant);
        a.provider = Some(p.provider);
        a.model = Some(caps.id.clone());
        a.effort = effective_effort;
        a.mode = Some(p.mode);
        a.status = "streaming".into();
        emit(AgentEvent::Message { message: a.clone() });
        let aid = a.id.clone();
        let emit2 = emit.clone();
        let mut on = move |ev: StreamEvent| match ev {
            StreamEvent::Text { delta } => emit2(AgentEvent::Delta { message_id: aid.clone(), kind: "text", text: delta }),
            StreamEvent::Reasoning { delta } => emit2(AgentEvent::Delta { message_id: aid.clone(), kind: "reasoning", text: delta }),
            StreamEvent::ToolStart { id, name } => {
                emit2(AgentEvent::ToolStatus { message_id: aid.clone(), call_id: id, name, status: "pending".into(), approval: None })
            }
            StreamEvent::Notice { message } => emit2(AgentEvent::Notice { code: "provider".into(), detail: message }),
        };
        let res = providers::run_turn(&state.http, &endpoint, &req, cancel, &mut on).await;
        let out = match res {
            Ok(TurnResult::Done(o)) => o,
            Ok(TurnResult::Stopped(o)) => {
                a.text = o.text;
                a.reasoning = o.reasoning;
                a.status = "partial".into();
                state.db.upsert_message(&p.conversation_id, &a)?;
                emit(AgentEvent::Message { message: a });
                return Ok(true);
            }
            Err(AppError::Cancelled) => {
                a.status = "partial".into();
                state.db.upsert_message(&p.conversation_id, &a)?;
                emit(AgentEvent::Message { message: a });
                return Ok(true);
            }
            Err(e) => {
                crate::logging::warn(&format!("provider error: {}", short_err(&e)));
                a.status = "error".into();
                a.error = serde_json::to_value(&e).ok();
                state.db.upsert_message(&p.conversation_id, &a)?;
                emit(AgentEvent::Message { message: a.clone() });
                emit(AgentEvent::Error { message_id: Some(a.id), error: e });
                return Ok(false);
            }
        };

        let mut usage = out.usage.clone();
        if caps.min_cache_tokens > 0 && usage.input_tokens < caps.min_cache_tokens {
            usage.eligible_input_tokens = None; // below the provider's cacheable minimum
        }
        a.text = out.text;
        a.reasoning = out.reasoning;
        a.tool_calls = out.tool_calls;
        a.raw = (!out.raw.is_null()).then(|| RawReplay { provider: p.provider, model: caps.id.clone(), data: out.raw });
        a.usage = Some(usage.clone());
        a.status = "complete".into();
        state.db.upsert_message(&p.conversation_id, &a)?;
        let total = state.db.add_usage(&p.conversation_id, &usage)?;
        emit(AgentEvent::Message { message: a.clone() });
        emit(AgentEvent::Usage { turn: usage, total });

        match out.stop {
            StopKind::Refusal => {
                emit(AgentEvent::Notice { code: "refusal".into(), detail: String::new() });
                return Ok(false);
            }
            StopKind::MaxTokens if a.tool_calls.is_empty() => {
                emit(AgentEvent::Notice { code: "max_tokens".into(), detail: String::new() });
                return Ok(false);
            }
            _ => {}
        }
        if a.tool_calls.is_empty() {
            return Ok(false);
        }

        // ---- tool execution (sequential, each may need approval) ----
        let mut results_msg = Message::new(Role::Tool);
        let mut stopped = false;
        for call in &a.tool_calls {
            if stopped || cancel.is_cancelled() {
                stopped = true;
                results_msg.tool_results.push(ToolResult {
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    content: "Cancelled by the user before running.".into(),
                    is_error: true,
                    status: "cancelled".into(),
                    diff: None,
                    images: vec![],
                });
                emit(tool_status(&a.id, call, "cancelled", None));
                continue;
            }
            let ctx = ToolCtx { mode: p.mode, workspace: ws_path.as_deref(), skills: &skills, images: image_ctx.clone() };
            let r = run_tool(state, &p.conversation_id, &a.id, call, &ctx, cancel, emit).await;
            if r.status == "cancelled" {
                stopped = true;
            }
            crate::logging::info(&format!("tool {} -> {}", call.name, r.status));
            results_msg.tool_results.push(r);
        }
        state.db.upsert_message(&p.conversation_id, &results_msg)?;
        emit(AgentEvent::Message { message: results_msg.clone() });
        messages.push(a);
        messages.push(results_msg);
        if stopped {
            return Ok(true);
        }
    }
    emit(AgentEvent::Notice { code: "max_steps".into(), detail: max_steps.to_string() });
    Ok(false)
}

fn short_err(e: &AppError) -> String {
    match e {
        AppError::Provider(p) => format!("{:?} status={:?}", p.kind, p.status),
        other => format!("{other}"),
    }
}

fn tool_status(mid: &str, call: &ToolCall, status: &str, approval: Option<ApprovalPrompt>) -> AgentEvent {
    AgentEvent::ToolStatus { message_id: mid.into(), call_id: call.id.clone(), name: call.name.clone(), status: status.into(), approval }
}

fn result(call: &ToolCall, content: String, status: &str, diff: Option<String>) -> ToolResult {
    ToolResult { call_id: call.id.clone(), name: call.name.clone(), is_error: status != "success", content, status: status.into(), diff, images: vec![] }
}

async fn run_tool(state: &Arc<AppState>, conv: &str, mid: &str, call: &ToolCall, ctx: &ToolCtx<'_>, cancel: &Cancel, emit: &Emit) -> ToolResult {
    let prepared = match tools::prepare(&call.name, &call.arguments, ctx) {
        Ok(p) => p,
        Err(e) => {
            emit(tool_status(mid, call, "failed", None));
            return result(call, format!("Error: {e}"), "failed", None);
        }
    };
    let diff = prepared.approval.as_ref().and_then(|a| a.diff.clone());
    if let Some(req) = prepared.approval
        && !session_allows(state, conv, ctx.workspace, &req) {
            let request_id = uuid::Uuid::new_v4().to_string();
            let (tx, rx) = oneshot::channel();
            lock(&state.approvals).insert(request_id.clone(), tx);
            emit(tool_status(mid, call, "pending", Some(ApprovalPrompt { request_id: request_id.clone(), request: req.clone() })));
            let decision = tokio::select! {
                d = rx => d.unwrap_or(Decision::Deny),
                _ = cancel.cancelled() => {
                    lock(&state.approvals).remove(&request_id);
                    emit(tool_status(mid, call, "cancelled", None));
                    return result(call, "Cancelled by the user.".into(), "cancelled", diff);
                }
            };
            match decision {
                Decision::Deny => {
                    emit(tool_status(mid, call, "cancelled", None));
                    return result(call, "The user denied this action. Do not retry it another way; ask the user how to proceed.".into(), "cancelled", diff);
                }
                Decision::AllowSession => remember(state, conv, ctx.workspace, &req),
                Decision::Allow => {}
            }
        }
    emit(tool_status(mid, call, "running", None));
    match tools::execute_full(prepared.action, cancel).await {
        Ok((out, images)) => {
            emit(tool_status(mid, call, "success", None));
            let mut r = result(call, out, "success", diff);
            r.images = images;
            r
        }
        Err(AppError::Cancelled) => {
            emit(tool_status(mid, call, "cancelled", None));
            result(call, "Cancelled by the user.".into(), "cancelled", diff)
        }
        Err(e) => {
            emit(tool_status(mid, call, "failed", None));
            result(call, format!("Error: {e}"), "failed", diff)
        }
    }
}

fn session_allows(state: &AppState, conv: &str, workspace: Option<&std::path::Path>, req: &ApprovalRequest) -> bool {
    // "Edit within workspace" mode: file edits (always inside the workspace by
    // construction) need no prompt. Commands and outside reads still ask.
    if req.kind == ApprovalKind::WriteFile && workspace.is_some() && state.settings().auto_approve_edits {
        return true;
    }
    let perms = lock(&state.perms);
    let Some(p) = perms.get(conv) else { return false };
    match req.kind {
        ApprovalKind::WriteFile => workspace.is_some_and(|w| p.writes.contains(w)),
        ApprovalKind::RunCommand => workspace.is_some_and(|w| p.commands.contains(&(w.to_path_buf(), req.target.clone()))),
        ApprovalKind::ReadOutside => p.outside_reads.contains(&req.target),
    }
}

fn remember(state: &AppState, conv: &str, workspace: Option<&std::path::Path>, req: &ApprovalRequest) {
    let mut perms = lock(&state.perms);
    let p = perms.entry(conv.to_string()).or_default();
    match req.kind {
        ApprovalKind::WriteFile => { if let Some(w) = workspace { p.writes.insert(w.to_path_buf()); } },
        ApprovalKind::RunCommand => {
            if let Some(w) = workspace { p.commands.insert((w.to_path_buf(), req.target.clone())); }
        }
        ApprovalKind::ReadOutside => {
            p.outside_reads.insert(req.target.clone());
        }
    }
}

pub fn respond_approval(state: &AppState, request_id: &str, d: Decision) -> AppResult<()> {
    let tx = lock(&state.approvals).remove(request_id).ok_or_else(|| AppError::NotFound("approval request expired".into()))?;
    let _ = tx.send(d);
    Ok(())
}

/// Used by commands when there is no stream to attach an error to.
pub fn provider_missing_key() -> AppError {
    ProviderError::new(ProviderErrorKind::MissingKey, "no API key configured").into()
}

#[cfg(test)]
mod tests;
