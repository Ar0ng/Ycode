//! Process-wide application state shared by commands and agent runs.

use crate::error::{AppError, AppResult};
use crate::model::ProviderId;
use crate::providers::registry::{self, ModelCaps};
use crate::providers::{Cancel, Endpoint};
use crate::secrets;
use crate::skills::SkillStore;
use crate::storage::{Db, Settings};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};
use tokio::sync::oneshot;

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    /// Allow this kind of action for the rest of the conversation (same command
    /// text for terminal commands; all workspace writes for file edits).
    AllowSession,
    Deny,
}

#[derive(Default)]
pub struct SessionPerms {
    pub writes: bool,
    pub commands: HashSet<String>,
    pub outside_reads: HashSet<String>,
}

pub struct AppState {
    pub db: Db,
    pub http: reqwest::Client,
    pub data_dir: PathBuf,
    pub settings: Mutex<Settings>,
    pub skills: RwLock<SkillStore>,
    pub runs: Mutex<HashMap<String, Cancel>>,
    pub approvals: Mutex<HashMap<String, oneshot::Sender<Decision>>>,
    pub perms: Mutex<HashMap<String, SessionPerms>>,
    /// Keys injected by tests instead of Credential Manager.
    pub key_override: Mutex<HashMap<ProviderId, String>>,
}

pub fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl AppState {
    pub fn new(db: Db, data_dir: PathBuf) -> Self {
        let settings = db.load_settings();
        let skills = SkillStore::load(&data_dir.join("skills"), &settings.disabled_skills);
        Self {
            db,
            http: crate::providers::http_client(),
            data_dir,
            settings: Mutex::new(settings),
            skills: RwLock::new(skills),
            runs: Mutex::new(HashMap::new()),
            approvals: Mutex::new(HashMap::new()),
            perms: Mutex::new(HashMap::new()),
            key_override: Mutex::new(HashMap::new()),
        }
    }

    pub fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }

    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) -> AppResult<Settings> {
        let mut g = lock(&self.settings);
        let mut next = g.clone();
        f(&mut next);
        self.db.save_settings(&next)?;
        *g = next.clone();
        Ok(next)
    }

    pub fn skills_snapshot(&self) -> SkillStore {
        self.skills.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Built-in table + last /models refresh + manually added IDs.
    pub fn model_table(&self, s: &Settings) -> Vec<ModelCaps> {
        let mut table = registry::builtin();
        for p in ProviderId::ALL {
            if let Some(remote) = s.remote_models.get(p.as_str()) {
                registry::merge_remote(&mut table, p, remote.clone());
            }
            for id in s.manual_models.get(p.as_str()).into_iter().flatten() {
                if !table.iter().any(|c| c.provider == p && &c.id == id) {
                    let mut c = registry::unknown(p, id);
                    c.source = "manual".into();
                    table.push(c);
                }
            }
        }
        table
    }

    pub fn caps_for(&self, s: &Settings, p: ProviderId, model: &str) -> ModelCaps {
        self.model_table(s).into_iter().find(|c| c.provider == p && c.id == model).unwrap_or_else(|| registry::unknown(p, model))
    }

    pub fn endpoint(&self, p: ProviderId) -> AppResult<Endpoint> {
        let s = self.settings();
        let base_url = s.base_urls.get(p.as_str()).filter(|u| !u.trim().is_empty()).cloned().unwrap_or_else(|| p.default_base_url().to_string());
        let over = lock(&self.key_override).get(&p).cloned();
        let api_key = match over {
            Some(k) => k,
            // Tests never touch the real Credential Manager.
            None if cfg!(test) => String::new(),
            None => secrets::get_key(p)?.unwrap_or_default(),
        };
        Ok(Endpoint { base_url, api_key })
    }

    pub fn has_key(&self, p: ProviderId) -> bool {
        lock(&self.key_override).contains_key(&p) || (!cfg!(test) && secrets::has_key(p))
    }

    pub fn register_run(&self, conv: &str) -> AppResult<Cancel> {
        let mut runs = lock(&self.runs);
        if runs.contains_key(conv) {
            return Err(AppError::Invalid("a response is already being generated in this conversation".into()));
        }
        let c = Cancel::default();
        runs.insert(conv.to_string(), c.clone());
        Ok(c)
    }

    pub fn finish_run(&self, conv: &str) {
        lock(&self.runs).remove(conv);
    }
}

/// Validate a user-entered base URL: https only (http allowed for localhost
/// testing), no credentials or query in the URL.
pub fn validate_base_url(u: &str) -> AppResult<String> {
    let u = u.trim().trim_end_matches('/');
    if u.is_empty() {
        return Ok(String::new());
    }
    let url = reqwest::Url::parse(u).map_err(|e| AppError::Invalid(format!("invalid URL: {e}")))?;
    let local = matches!(url.host_str(), Some("localhost") | Some("127.0.0.1") | Some("[::1]"));
    if url.scheme() != "https" && !(url.scheme() == "http" && local) {
        return Err(AppError::Invalid("base URL must use https".into()));
    }
    if !url.username().is_empty() || url.password().is_some() || url.query().is_some() {
        return Err(AppError::Invalid("base URL must not contain credentials or a query".into()));
    }
    Ok(u.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_url_validation() {
        assert_eq!(validate_base_url("https://api.example.com/v1/").unwrap(), "https://api.example.com/v1");
        assert!(validate_base_url("http://api.example.com").is_err());
        assert!(validate_base_url("http://127.0.0.1:8080").is_ok());
        assert!(validate_base_url("https://user:pw@x.com").is_err());
        assert_eq!(validate_base_url("").unwrap(), "");
    }
}
