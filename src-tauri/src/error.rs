//! Application error type. Every error that crosses the IPC boundary is turned into
//! `{ code, message }` so the UI can localise the code and show a short message
//! instead of a raw stack trace.

use serde::Serialize;

#[derive(Debug, Clone, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Permission(String),
    #[error("{0}")]
    Io(String),
    #[error("{0}")]
    Db(String),
    #[error("{0}")]
    Secret(String),
    #[error("{0}")]
    Skill(String),
    #[error("{0}")]
    Update(String),
    #[error("{0}")]
    Provider(ProviderError),
    #[error("cancelled")]
    Cancelled,
}

/// Classified provider failure. `kind` is stable and localised by the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ProviderError {
    pub kind: ProviderErrorKind,
    pub status: Option<u16>,
    /// Short vendor message (already stripped of anything that could contain a key).
    pub detail: String,
    pub retryable: bool,
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.detail)
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorKind {
    MissingKey,
    InvalidKey,
    InsufficientBalance,
    ModelNotFound,
    ModelNoAccess,
    RateLimited,
    Overloaded,
    Timeout,
    Network,
    StreamInterrupted,
    BadRequest,
    Refused,
    Server,
    Unknown,
}

impl ProviderError {
    pub fn new(kind: ProviderErrorKind, detail: impl Into<String>) -> Self {
        let retryable = matches!(
            kind,
            ProviderErrorKind::RateLimited
                | ProviderErrorKind::Overloaded
                | ProviderErrorKind::Timeout
                | ProviderErrorKind::Network
                | ProviderErrorKind::StreamInterrupted
                | ProviderErrorKind::Server
        );
        Self { kind, status: None, detail: truncate(&detail.into(), 400), retryable }
    }

    /// Classify an HTTP error response. `body` is the vendor's JSON error body.
    pub fn from_status(status: u16, body: &str) -> Self {
        let lower = body.to_ascii_lowercase();
        let detail = extract_message(body);
        let kind = match status {
            401 => ProviderErrorKind::InvalidKey,
            402 => ProviderErrorKind::InsufficientBalance,
            403 => {
                if lower.contains("model") { ProviderErrorKind::ModelNoAccess } else { ProviderErrorKind::InvalidKey }
            }
            404 => ProviderErrorKind::ModelNotFound,
            408 => ProviderErrorKind::Timeout,
            429 => {
                if lower.contains("insufficient_quota") || lower.contains("quota") || lower.contains("balance") {
                    ProviderErrorKind::InsufficientBalance
                } else {
                    ProviderErrorKind::RateLimited
                }
            }
            400 | 422 => {
                if lower.contains("model") && (lower.contains("not exist") || lower.contains("not found") || lower.contains("does not exist")) {
                    ProviderErrorKind::ModelNotFound
                } else if lower.contains("credit balance") || lower.contains("insufficient") {
                    ProviderErrorKind::InsufficientBalance
                } else {
                    ProviderErrorKind::BadRequest
                }
            }
            529 | 503 => ProviderErrorKind::Overloaded,
            500..=599 => ProviderErrorKind::Server,
            _ => ProviderErrorKind::Unknown,
        };
        let mut e = Self::new(kind, detail);
        e.status = Some(status);
        e
    }
}

fn extract_message(body: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(body) {
        let m = v.pointer("/error/message").or_else(|| v.get("message")).and_then(|m| m.as_str());
        if let Some(m) = m {
            return m.to_string();
        }
    }
    body.chars().take(300).collect()
}

pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

#[derive(Serialize)]
struct Wire<'a> {
    code: &'a str,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<&'a ProviderError>,
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let code = match self {
            AppError::Invalid(_) => "invalid",
            AppError::NotFound(_) => "not_found",
            AppError::Permission(_) => "permission",
            AppError::Io(_) => "io",
            AppError::Db(_) => "db",
            AppError::Secret(_) => "secret",
            AppError::Skill(_) => "skill",
            AppError::Update(_) => "update",
            AppError::Provider(_) => "provider",
            AppError::Cancelled => "cancelled",
        };
        let provider = if let AppError::Provider(p) = self { Some(p) } else { None };
        Wire { code, message: self.to_string(), provider }.serialize(s)
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Io(e.to_string())
    }
}
impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Db(e.to_string())
    }
}
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Invalid(e.to_string())
    }
}
impl From<ProviderError> for AppError {
    fn from(e: ProviderError) -> Self {
        AppError::Provider(e)
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_statuses() {
        assert_eq!(ProviderError::from_status(401, "{}").kind, ProviderErrorKind::InvalidKey);
        assert_eq!(ProviderError::from_status(402, "{}").kind, ProviderErrorKind::InsufficientBalance);
        assert_eq!(
            ProviderError::from_status(429, r#"{"error":{"message":"You exceeded your current quota","type":"insufficient_quota"}}"#).kind,
            ProviderErrorKind::InsufficientBalance
        );
        assert_eq!(ProviderError::from_status(429, r#"{"error":{"message":"Rate limit"}}"#).kind, ProviderErrorKind::RateLimited);
        assert_eq!(
            ProviderError::from_status(400, r#"{"error":{"message":"The model `x` does not exist"}}"#).kind,
            ProviderErrorKind::ModelNotFound
        );
        assert_eq!(ProviderError::from_status(404, "{}").kind, ProviderErrorKind::ModelNotFound);
        assert_eq!(ProviderError::from_status(529, "{}").kind, ProviderErrorKind::Overloaded);
        assert!(ProviderError::from_status(500, "{}").retryable);
        assert!(!ProviderError::from_status(401, "{}").retryable);
    }

    #[test]
    fn extracts_vendor_message() {
        let e = ProviderError::from_status(400, r#"{"type":"error","error":{"type":"invalid_request_error","message":"bad thing"}}"#);
        assert_eq!(e.detail, "bad thing");
    }
}
