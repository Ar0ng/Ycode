//! Provider-neutral conversation model shared by storage, the agent loop and the
//! provider adapters. Provider-native data needed for faithful replay (encrypted
//! reasoning items, thinking blocks, `reasoning_content`) is kept in `raw` and only
//! re-sent to the same provider + model that produced it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Openai,
    Deepseek,
    Anthropic,
}

impl ProviderId {
    pub const ALL: [ProviderId; 3] = [ProviderId::Openai, ProviderId::Deepseek, ProviderId::Anthropic];
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderId::Openai => "openai",
            ProviderId::Deepseek => "deepseek",
            ProviderId::Anthropic => "anthropic",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "openai" => Some(Self::Openai),
            "deepseek" => Some(Self::Deepseek),
            "anthropic" => Some(Self::Anthropic),
            _ => None,
        }
    }
    pub fn default_base_url(&self) -> &'static str {
        match self {
            ProviderId::Openai => "https://api.openai.com/v1",
            ProviderId::Deepseek => "https://api.deepseek.com",
            ProviderId::Anthropic => "https://api.anthropic.com",
        }
    }
}

/// Unified reasoning-effort vocabulary. Each model supports a subset; the UI only
/// offers that subset and the adapters only send supported values.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl Effort {
    pub fn as_str(&self) -> &'static str {
        match self {
            Effort::None => "none",
            Effort::Minimal => "minimal",
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Xhigh => "xhigh",
            Effort::Max => "max",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "none" => Effort::None,
            "minimal" => Effort::Minimal,
            "low" => Effort::Low,
            "medium" => Effort::Medium,
            "high" => Effort::High,
            "xhigh" => Effort::Xhigh,
            "max" => Effort::Max,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Office,
    Coding,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    /// Results of the tool calls of the preceding assistant message.
    Tool,
    /// In-band control record (e.g. a mid-conversation effort change). Never shown as chat.
    Control,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Parsed JSON arguments (object). Invalid JSON is stored as `{"_raw": "..."}`.
    pub arguments: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub call_id: String,
    pub name: String,
    pub content: String,
    pub is_error: bool,
    /// UI-only status: success | failed | cancelled | denied
    pub status: String,
    /// Optional unified diff shown in the UI (never sent to the model separately).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    /// Ids of images produced by this call (files in `<app data>/images`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<String>,
}

/// Provider-native replay payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RawReplay {
    pub provider: ProviderId,
    pub model: String,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Control {
    /// Effort changed mid-conversation. `in_band` = sent as a protocol item that keeps
    /// the cached prefix (OpenAI `configuration_update`, Claude per-message effort).
    Effort { provider: ProviderId, model: String, effort: Effort, in_band: bool },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Usage {
    /// Total input tokens billed for the request, cached + uncached.
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    #[serde(default)]
    pub cache_write_tokens: Option<u64>,
    #[serde(default)]
    pub reasoning_tokens: Option<u64>,
    /// Denominator for the hit rate, per provider definition; `None` → show N/A.
    #[serde(default)]
    pub eligible_input_tokens: Option<u64>,
}

impl Usage {
    pub fn hit_rate(&self) -> Option<f64> {
        match self.eligible_input_tokens {
            Some(d) if d > 0 => Some(self.cached_input_tokens as f64 / d as f64),
            _ => None,
        }
    }
    pub fn add(&mut self, o: &Usage) {
        self.input_tokens += o.input_tokens;
        self.output_tokens += o.output_tokens;
        self.cached_input_tokens += o.cached_input_tokens;
        self.cache_write_tokens = match (self.cache_write_tokens, o.cache_write_tokens) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        };
        self.reasoning_tokens = match (self.reasoning_tokens, o.reasoning_tokens) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        };
        self.eligible_input_tokens = match (self.eligible_input_tokens, o.eligible_input_tokens) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        };
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Attachment {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub kind: AttachmentKind,
    /// Text content for supported text attachments (TXT / Markdown). Images, PDFs and
    /// Office files are not sent in V1 and are rejected before sending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AttachmentKind {
    Text,
    Image,
    Pdf,
    Office,
}

impl AttachmentKind {
    pub fn supported_in_v1(&self) -> bool {
        matches!(self, AttachmentKind::Text)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Message {
    pub id: String,
    pub role: Role,
    #[serde(default)]
    pub text: String,
    /// Displayed reasoning summary (never re-sent; replay uses `raw`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_results: Vec<ToolResult>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<RawReplay>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<Control>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<ProviderId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<Mode>,
    /// complete | partial (stopped by user) | error
    #[serde(default = "default_status")]
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
    pub created_at: i64,
}

fn default_status() -> String {
    "complete".into()
}

impl Message {
    pub fn new(role: Role) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            role,
            text: String::new(),
            reasoning: None,
            tool_calls: vec![],
            tool_results: vec![],
            attachments: vec![],
            raw: None,
            control: None,
            usage: None,
            provider: None,
            model: None,
            effort: None,
            mode: None,
            status: default_status(),
            error: None,
            created_at: now_ms(),
        }
    }

    /// Text sent to the model for a user message: the typed text followed by any
    /// supported text attachments. Deterministic so replays are byte-stable.
    pub fn user_content_text(&self) -> String {
        let mut s = self.text.clone();
        for a in &self.attachments {
            if let (AttachmentKind::Text, Some(t)) = (a.kind, &a.text) {
                s.push_str(&format!("\n\n<attachment name=\"{}\">\n{}\n</attachment>", a.name, t));
            }
        }
        s
    }

    /// Raw replay data if it was produced by exactly this provider + model.
    pub fn raw_for(&self, provider: ProviderId, model: &str) -> Option<&Value> {
        self.raw.as_ref().filter(|r| r.provider == provider && r.model == model).map(|r| &r.data)
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Stable 64-bit FNV-1a hash (used for cache keys / change detection; not security).
pub fn fnv64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
