//! Unified provider interface. The agent core only talks to `run_turn` /
//! `list_models`; each adapter owns its wire format, its cache parameters and its
//! effort parameters.

pub mod anthropic;
pub mod deepseek;
pub mod openai;
pub mod registry;
pub mod sse;

use crate::error::{AppError, ProviderError, ProviderErrorKind};
use crate::model::{Effort, Message, ProviderId, ToolCall, Usage};
use futures_util::StreamExt;
use registry::ModelCaps;
use serde::Serialize;
use serde_json::Value;
use sse::{SseEvent, SseParser};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::Notify;

/// Cooperative cancellation shared between the UI command and in-flight work.
#[derive(Clone, Default)]
pub struct Cancel(Arc<CancelInner>);

#[derive(Default)]
struct CancelInner {
    flag: AtomicBool,
    notify: Notify,
}

impl Cancel {
    pub fn cancel(&self) {
        self.0.flag.store(true, Ordering::SeqCst);
        self.0.notify.notify_waiters();
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.flag.load(Ordering::SeqCst)
    }
    pub async fn cancelled(&self) {
        loop {
            let n = self.0.notify.notified();
            if self.is_cancelled() {
                return;
            }
            n.await;
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value,
}

#[derive(Clone)]
pub struct Endpoint {
    pub base_url: String,
    pub api_key: String,
}

impl Endpoint {
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url.trim_end_matches('/'), path)
    }
}

pub struct TurnRequest<'a> {
    pub provider: ProviderId,
    pub model: &'a str,
    pub caps: &'a ModelCaps,
    /// Stable system blocks in fixed order (base, mode, skills index).
    pub system: &'a [String],
    pub tools: &'a [ToolSpec],
    pub history: &'a [Message],
    /// Request-level effort. For in-band capable models this is the effort the
    /// conversation started with; later changes travel as Control messages.
    pub effort: Option<Effort>,
    pub cache_key: &'a str,
    pub max_output: u64,
    pub fallbacks: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StreamEvent {
    Text { delta: String },
    Reasoning { delta: String },
    ToolStart { id: String, name: String },
    Notice { message: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StopKind {
    End,
    ToolUse,
    MaxTokens,
    Refusal,
    Other(String),
}

#[derive(Debug, Clone)]
pub struct TurnOutput {
    pub text: String,
    pub reasoning: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub raw: Value,
    pub usage: Usage,
    pub stop: StopKind,
}

pub trait Decoder: Send {
    fn on_event(&mut self, ev: SseEvent, out: &mut Vec<StreamEvent>) -> Result<(), ProviderError>;
    /// True once the protocol-level terminal event was seen.
    fn done(&self) -> bool;
    /// Content produced so far (used when the user stops generation).
    fn partial(&self) -> TurnOutput;
    fn finish(self: Box<Self>) -> Result<TurnOutput, ProviderError>;
}

pub struct BuiltRequest {
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

pub fn build(req: &TurnRequest) -> BuiltRequest {
    match req.provider {
        ProviderId::Openai => openai::build(req),
        ProviderId::Deepseek => deepseek::build(req),
        ProviderId::Anthropic => anthropic::build(req),
    }
}

fn decoder(p: ProviderId, model: &str) -> Box<dyn Decoder> {
    match p {
        ProviderId::Openai => Box::new(openai::OpenAiDecoder::new(model)),
        ProviderId::Deepseek => Box::new(deepseek::DeepSeekDecoder::new(model)),
        ProviderId::Anthropic => Box::new(anthropic::AnthropicDecoder::new(model)),
    }
}

fn auth_headers(p: ProviderId, key: &str) -> Vec<(String, String)> {
    match p {
        ProviderId::Anthropic => vec![
            ("x-api-key".into(), key.into()),
            ("anthropic-version".into(), anthropic::API_VERSION.into()),
        ],
        _ => vec![("authorization".into(), format!("Bearer {key}"))],
    }
}

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .pool_idle_timeout(Duration::from_secs(90))
        .user_agent(concat!("Ycode/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("http client")
}

/// Max silence between stream chunks before we treat the stream as dead.
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_RETRIES: u32 = 2;

/// Outcome of one streamed turn. `Stopped` carries what was produced before the user
/// pressed Stop so it can be kept.
pub enum TurnResult {
    Done(TurnOutput),
    Stopped(TurnOutput),
}

pub async fn run_turn(
    http: &reqwest::Client,
    ep: &Endpoint,
    req: &TurnRequest<'_>,
    cancel: &Cancel,
    on: &mut (dyn FnMut(StreamEvent) + Send),
) -> Result<TurnResult, AppError> {
    if ep.api_key.is_empty() {
        return Err(ProviderError::new(ProviderErrorKind::MissingKey, "no API key configured").into());
    }
    let built = build(req);
    let mut attempt = 0;
    loop {
        attempt += 1;
        match stream_once(http, ep, req, &built, cancel, on).await {
            Err(AppError::Provider(e)) if e.retryable && attempt <= MAX_RETRIES && !cancel.is_cancelled() => {
                // Retried only when nothing was streamed yet (stream_once marks
                // interrupted streams as non-retryable).
                let wait = Duration::from_millis(800 * 2u64.pow(attempt));
                on(StreamEvent::Notice { message: format!("retry:{}:{}", attempt, e.detail) });
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {}
                    _ = cancel.cancelled() => return Err(AppError::Cancelled),
                }
            }
            other => return other,
        }
    }
}

async fn stream_once(
    http: &reqwest::Client,
    ep: &Endpoint,
    req: &TurnRequest<'_>,
    built: &BuiltRequest,
    cancel: &Cancel,
    on: &mut (dyn FnMut(StreamEvent) + Send),
) -> Result<TurnResult, AppError> {
    let mut rb = http.post(ep.url(&built.path)).header("content-type", "application/json");
    for (k, v) in auth_headers(req.provider, &ep.api_key).into_iter().chain(built.headers.iter().cloned()) {
        rb = rb.header(k, v);
    }
    let send = rb.json(&built.body).send();
    let resp = tokio::select! {
        r = send => r.map_err(net_err)?,
        _ = cancel.cancelled() => return Err(AppError::Cancelled),
    };
    let status = resp.status().as_u16();
    if status >= 400 {
        let body = resp.text().await.unwrap_or_default();
        return Err(ProviderError::from_status(status, &body).into());
    }
    let mut dec = decoder(req.provider, req.model);
    let mut parser = SseParser::new();
    let mut stream = resp.bytes_stream();
    let mut events = Vec::new();
    let mut produced = false;
    loop {
        let next = tokio::select! {
            n = tokio::time::timeout(IDLE_TIMEOUT, stream.next()) => n,
            _ = cancel.cancelled() => {
                return Ok(TurnResult::Stopped(dec.partial()));
            }
        };
        let chunk = match next {
            Err(_) => return Err(interrupted(produced, ProviderErrorKind::Timeout, "stream idle timeout")),
            Ok(None) => break,
            Ok(Some(Err(e))) => return Err(interrupted(produced, ProviderErrorKind::StreamInterrupted, &e.to_string())),
            Ok(Some(Ok(b))) => b,
        };
        for ev in parser.feed(&chunk) {
            dec.on_event(ev, &mut events)?;
        }
        if !events.is_empty() {
            produced = true;
            for e in events.drain(..) {
                on(e);
            }
        }
        if dec.done() {
            break;
        }
    }
    for ev in parser.finish() {
        dec.on_event(ev, &mut events)?;
    }
    for e in events.drain(..) {
        on(e);
    }
    if !dec.done() {
        return Err(interrupted(produced, ProviderErrorKind::StreamInterrupted, "stream ended before completion"));
    }
    Ok(TurnResult::Done(dec.finish()?))
}

fn interrupted(produced: bool, kind: ProviderErrorKind, detail: &str) -> AppError {
    let mut e = ProviderError::new(kind, detail);
    if produced {
        // Output already reached the UI: don't silently retry and duplicate it.
        e.retryable = false;
    }
    e.into()
}

fn net_err(e: reqwest::Error) -> AppError {
    let kind = if e.is_timeout() { ProviderErrorKind::Timeout } else { ProviderErrorKind::Network };
    // reqwest errors include the URL but never headers, so no key can leak here.
    ProviderError::new(kind, e.without_url().to_string()).into()
}

/// GET helper for list-models / connection tests.
pub async fn get_json(http: &reqwest::Client, ep: &Endpoint, provider: ProviderId, path: &str) -> Result<Value, AppError> {
    if ep.api_key.is_empty() {
        return Err(ProviderError::new(ProviderErrorKind::MissingKey, "no API key configured").into());
    }
    let mut rb = http.get(ep.url(path)).timeout(Duration::from_secs(30));
    for (k, v) in auth_headers(provider, &ep.api_key) {
        rb = rb.header(k, v);
    }
    let resp = rb.send().await.map_err(net_err)?;
    let status = resp.status().as_u16();
    let body = resp.text().await.map_err(net_err)?;
    if status >= 400 {
        return Err(ProviderError::from_status(status, &body).into());
    }
    serde_json::from_str(&body).map_err(|e| ProviderError::new(ProviderErrorKind::Unknown, e.to_string()).into())
}

pub async fn list_models(http: &reqwest::Client, ep: &Endpoint, provider: ProviderId) -> Result<Vec<ModelCaps>, AppError> {
    match provider {
        ProviderId::Openai => Ok(openai::parse_models(&get_json(http, ep, provider, "/models").await?)),
        ProviderId::Deepseek => Ok(deepseek::parse_models(&get_json(http, ep, provider, "/models").await?)),
        ProviderId::Anthropic => {
            let mut out = Vec::new();
            let mut after: Option<String> = None;
            for _ in 0..10 {
                let path = match &after {
                    Some(a) => format!("/v1/models?limit=1000&after_id={a}"),
                    None => "/v1/models?limit=1000".to_string(),
                };
                let v = get_json(http, ep, provider, &path).await?;
                out.extend(anthropic::parse_models(&v));
                if v.get("has_more").and_then(|h| h.as_bool()) == Some(true) {
                    after = v.get("last_id").and_then(|s| s.as_str()).map(String::from);
                    if after.is_none() {
                        break;
                    }
                } else {
                    break;
                }
            }
            Ok(out)
        }
    }
}

/// Parse tool-call argument JSON; invalid JSON is preserved so the agent can report
/// a precise error back to the model instead of crashing.
pub fn parse_args(s: &str) -> Value {
    if s.trim().is_empty() {
        return Value::Object(Default::default());
    }
    serde_json::from_str::<Value>(s).unwrap_or_else(|_| serde_json::json!({ "_raw": s }))
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;
    use crate::model::{Message, Role, ToolResult};

    pub fn caps(p: ProviderId, id: &str) -> ModelCaps {
        registry::builtin().into_iter().find(|c| c.provider == p && c.id == id).unwrap()
    }

    pub fn sample_history(p: ProviderId, model: &str) -> Vec<Message> {
        let mut u = Message::new(Role::User);
        u.text = "List files".into();
        let mut a = Message::new(Role::Assistant);
        a.text = "Looking.".into();
        a.tool_calls = vec![ToolCall { id: "call_1".into(), name: "list_dir".into(), arguments: serde_json::json!({"path": "."}) }];
        a.provider = Some(p);
        a.model = Some(model.into());
        let mut t = Message::new(Role::Tool);
        t.tool_results = vec![ToolResult {
            call_id: "call_1".into(),
            name: "list_dir".into(),
            content: "a.txt".into(),
            is_error: false,
            status: "success".into(),
            diff: None,
            images: vec![],
        }];
        vec![u, a, t]
    }

    pub fn req<'a>(p: ProviderId, caps: &'a ModelCaps, system: &'a [String], tools: &'a [ToolSpec], h: &'a [Message]) -> TurnRequest<'a> {
        TurnRequest {
            provider: p,
            model: &caps.id,
            caps,
            system,
            tools,
            history: h,
            effort: caps.default_effort,
            cache_key: "ycode-test",
            max_output: 32000,
            fallbacks: true,
        }
    }

    /// The serialized request for turn N must be a byte prefix (modulo the closing
    /// brackets) of the request for turn N+1 in the conversation part — this is the
    /// property provider prefix caches depend on.
    pub fn messages_prefix_stable(a: &Value, b: &Value, key: &str) -> bool {
        let (Some(x), Some(y)) = (a.get(key).and_then(|v| v.as_array()), b.get(key).and_then(|v| v.as_array())) else {
            return false;
        };
        x.len() <= y.len() && x.iter().zip(y.iter()).all(|(p, q)| p == q)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn cancel_wakes_waiters() {
        let c = Cancel::default();
        let c2 = c.clone();
        let h = tokio::spawn(async move { c2.cancelled().await });
        tokio::time::sleep(Duration::from_millis(10)).await;
        c.cancel();
        tokio::time::timeout(Duration::from_secs(1), h).await.unwrap().unwrap();
        // Already-cancelled returns immediately.
        tokio::time::timeout(Duration::from_millis(50), c.cancelled()).await.unwrap();
    }

    #[test]
    fn parse_args_keeps_invalid_json() {
        assert_eq!(parse_args("{\"a\":1}")["a"], 1);
        assert_eq!(parse_args("{bad")["_raw"], "{bad");
        assert!(parse_args("").is_object());
    }
}
