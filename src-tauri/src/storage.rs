//! Local persistence: one SQLite file (WAL) in the app data folder.
//! Conversations and messages are append-only rows; settings are one JSON document.
//! API keys are NOT stored here (see `secrets.rs`).

use crate::error::{AppError, AppResult};
use crate::model::{Effort, Message, Mode, ProviderId, Usage, now_ms};
use crate::providers::registry::ModelCaps;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const SCHEMA_VERSION: i64 = 3;

/// GitHub repository the updater checks by default (editable in Settings › Updates).
pub const DEFAULT_UPDATE_REPO: &str = "Ar0ng/Ycode";

/// Ordered list of migrations; index i migrates from version i to i+1.
const MIGRATIONS: &[&str] = &[
    // v0 -> v1
    "CREATE TABLE IF NOT EXISTS conversations (
        id TEXT PRIMARY KEY,
        title TEXT NOT NULL DEFAULT '',
        mode TEXT NOT NULL,
        workspace TEXT,
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL,
        usage_json TEXT
    );
    CREATE TABLE IF NOT EXISTS messages (
        id TEXT PRIMARY KEY,
        conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
        seq INTEGER NOT NULL,
        json TEXT NOT NULL,
        UNIQUE(conversation_id, seq)
    );
    CREATE INDEX IF NOT EXISTS idx_conv_updated ON conversations(updated_at DESC);
    CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    // v1 -> v2: conversation groups (sidebar sections). Existing chats stay ungrouped.
    "CREATE TABLE IF NOT EXISTS conv_groups (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        sort INTEGER NOT NULL DEFAULT 0,
        collapsed INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL
    );
    ALTER TABLE conversations ADD COLUMN group_id TEXT;",
    // v2 -> v3: per-conversation model settings (provider, model, effort, mode).
    "ALTER TABLE conversations ADD COLUMN config_json TEXT;",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConversationGroup {
    pub id: String,
    pub name: String,
    pub sort: i64,
    pub collapsed: bool,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub mode: Mode,
    pub workspace: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub group_id: Option<String>,
    /// Model settings of this chat; None for chats from before 0.2.3.
    #[serde(default)]
    pub config: Option<crate::model::ConvConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    #[serde(flatten)]
    pub summary: ConversationSummary,
    pub messages: Vec<Message>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "zh-CN" | "en-US"; None = follow system on first start.
    pub language: Option<String>,
    /// "system" | "light" | "dark"
    pub theme: String,
    /// Display detail for agent activity; this does not change tool permissions.
    pub activity_display: String,
    pub provider: ProviderId,
    pub mode: Mode,
    /// Selected model per provider.
    pub models: BTreeMap<String, String>,
    /// Last effort per "provider:model".
    pub efforts: BTreeMap<String, Effort>,
    /// Custom base URL per provider (absent = official default).
    pub base_urls: BTreeMap<String, String>,
    /// Models the user typed by hand, per provider.
    pub manual_models: BTreeMap<String, Vec<String>>,
    /// Last refreshed /models result per provider.
    pub remote_models: BTreeMap<String, Vec<ModelCaps>>,
    pub remote_refreshed_at: BTreeMap<String, i64>,
    pub workspace: Option<String>,
    pub recent_workspaces: Vec<String>,
    pub disabled_skills: Vec<String>,
    /// GitHub "owner/repo" used by the updater; empty = updater not configured.
    pub update_repo: String,
    /// On: updates always come from the official Ar0ng/Ycode. Off: use `update_repo`.
    pub auto_update_repo: bool,
    pub check_updates_on_start: bool,
    /// Display name used in greetings; empty = the Windows user name.
    pub profile_name: String,
    /// Approval mode: true = file edits inside the workspace are applied without
    /// asking ("edit within workspace"); commands and outside reads still ask.
    pub auto_approve_edits: bool,
    pub claude_fallbacks: bool,
    pub sidebar_collapsed: bool,
    /// Max agent steps (model calls) per user message.
    pub max_steps: u32,
    /// Offer GPT Image tools (needs an OpenAI key; billed by OpenAI).
    pub image_generation: bool,
    pub image_model: String,
    /// Hide older model generations in the model picker (e.g. GPT below 5.5, Claude 4.x).
    pub hide_legacy_models: bool,
    /// Models hidden by hand, as "provider:model".
    pub hidden_models: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: None,
            theme: "system".into(),
            activity_display: "minimal".into(),
            provider: ProviderId::Openai,
            mode: Mode::Office,
            models: BTreeMap::new(),
            efforts: BTreeMap::new(),
            base_urls: BTreeMap::new(),
            manual_models: BTreeMap::new(),
            remote_models: BTreeMap::new(),
            remote_refreshed_at: BTreeMap::new(),
            workspace: None,
            recent_workspaces: vec![],
            disabled_skills: vec![],
            update_repo: DEFAULT_UPDATE_REPO.into(),
            auto_update_repo: true,
            check_updates_on_start: true,
            profile_name: String::new(),
            auto_approve_edits: false,
            claude_fallbacks: true,
            sidebar_collapsed: false,
            max_steps: 40,
            image_generation: true,
            image_model: crate::images::DEFAULT_MODEL.into(),
            hide_legacy_models: true,
            hidden_models: vec![],
        }
    }
}

impl Settings {
    /// Fill in values that must never be empty. Settings saved by versions before
    /// 0.1.1 have `update_repo: ""`, which left the updater unconfigured after an
    /// upgrade; an empty repository always means the official one.
    pub fn normalize(&mut self) {
        if self.activity_display != "full" { self.activity_display = "minimal".into(); }
        let repo = self.update_repo.trim().trim_matches('/');
        self.update_repo = if repo.is_empty() { DEFAULT_UPDATE_REPO.to_string() } else { repo.to_string() };
        self.profile_name = self.profile_name.trim().chars().filter(|c| !c.is_control()).take(32).collect();
    }

    /// Repository the updater actually uses.
    pub fn effective_update_repo(&self) -> &str {
        if self.auto_update_repo { DEFAULT_UPDATE_REPO } else { &self.update_repo }
    }
}

pub struct Db {
    conn: Mutex<Connection>,
    /// Set when the DB or settings had to be recovered on startup.
    pub recovery_notice: Mutex<Option<String>>,
}

impl Db {
    pub fn open(path: &Path) -> AppResult<Self> {
        let mut notice = None;
        let conn = match Self::try_open(path) {
            Ok(c) => c,
            Err(e) => {
                // Corrupt or unreadable database: keep the bad file for manual
                // recovery and start fresh instead of refusing to launch.
                let backup = path.with_extension(format!("corrupt-{}.db", now_ms()));
                let _ = std::fs::rename(path, &backup);
                for ext in ["db-wal", "db-shm"] {
                    let _ = std::fs::remove_file(path.with_extension(ext));
                }
                notice = Some(format!("database recovered; previous file kept at {} ({e})", backup.display()));
                Self::try_open(path)?
            }
        };
        Ok(Self { conn: Mutex::new(conn), recovery_notice: Mutex::new(notice) })
    }

    pub fn open_in_memory() -> AppResult<Self> {
        let conn = Connection::open_in_memory()?;
        migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn), recovery_notice: Mutex::new(None) })
    }

    fn try_open(path: &Path) -> AppResult<Connection> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let ok: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if ok != "ok" {
            return Err(AppError::Db(format!("integrity check failed: {ok}")));
        }
        migrate(&conn)?;
        Ok(conn)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    // ---------- settings ----------

    pub fn load_settings(&self) -> Settings {
        let c = self.lock();
        let raw: Option<String> = c.query_row("SELECT value FROM kv WHERE key='settings'", [], |r| r.get(0)).optional().ok().flatten();
        let mut s = match raw {
            None => Settings::default(),
            Some(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                // Corrupt settings: fall back to defaults but keep a copy.
                let _ = c.execute("INSERT OR REPLACE INTO kv(key,value) VALUES('settings_corrupt',?1)", params![s]);
                *self.recovery_notice.lock().unwrap_or_else(|p| p.into_inner()) = Some(format!("settings were reset ({e})"));
                Settings::default()
            }),
        };
        s.normalize();
        s
    }

    /// Profile picture as a small `data:image/...;base64,` URL (stored apart from
    /// settings so the settings document stays small).
    pub fn load_avatar(&self) -> Option<String> {
        self.lock().query_row("SELECT value FROM kv WHERE key='avatar'", [], |r| r.get(0)).optional().ok().flatten()
    }

    pub fn save_avatar(&self, data_url: Option<&str>) -> AppResult<()> {
        let c = self.lock();
        match data_url {
            Some(d) => c.execute("INSERT OR REPLACE INTO kv(key,value) VALUES('avatar',?1)", params![d])?,
            None => c.execute("DELETE FROM kv WHERE key='avatar'", [])?,
        };
        Ok(())
    }

    pub fn save_settings(&self, s: &Settings) -> AppResult<()> {
        let json = serde_json::to_string(s)?;
        self.lock().execute("INSERT OR REPLACE INTO kv(key,value) VALUES('settings',?1)", params![json])?;
        Ok(())
    }

    // ---------- conversations ----------

    pub fn create_conversation(&self, id: &str, mode: Mode, workspace: Option<&str>) -> AppResult<ConversationSummary> {
        self.create_conversation_in(id, mode, workspace, None)
    }

    pub fn create_conversation_in(&self, id: &str, mode: Mode, workspace: Option<&str>, group: Option<&str>) -> AppResult<ConversationSummary> {
        let now = now_ms();
        let c = self.lock();
        // Ignore a group id that doesn't exist (e.g. deleted meanwhile).
        let group: Option<String> = match group {
            Some(g) => c.query_row("SELECT id FROM conv_groups WHERE id=?1", params![g], |r| r.get(0)).optional()?,
            None => None,
        };
        c.execute(
            "INSERT INTO conversations(id,title,mode,workspace,created_at,updated_at,group_id) VALUES(?1,'',?2,?3,?4,?4,?5)",
            params![id, mode_str(mode), workspace, now, group],
        )?;
        Ok(ConversationSummary { id: id.into(), title: String::new(), mode, workspace: workspace.map(String::from), created_at: now, updated_at: now, group_id: group, config: None })
    }

    // ---------- groups ----------

    pub fn list_groups(&self) -> AppResult<Vec<ConversationGroup>> {
        let c = self.lock();
        let mut st = c.prepare("SELECT id,name,sort,collapsed,created_at FROM conv_groups ORDER BY sort, created_at")?;
        let rows = st.query_map([], |r| Ok(ConversationGroup { id: r.get(0)?, name: r.get(1)?, sort: r.get(2)?, collapsed: r.get::<_, i64>(3)? != 0, created_at: r.get(4)? }))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn create_group(&self, name: &str) -> AppResult<ConversationGroup> {
        let name = clean_group_name(name)?;
        let c = self.lock();
        let sort: i64 = c.query_row("SELECT COALESCE(MAX(sort),-1)+1 FROM conv_groups", [], |r| r.get(0))?;
        let g = ConversationGroup { id: uuid::Uuid::new_v4().to_string(), name, sort, collapsed: false, created_at: now_ms() };
        c.execute("INSERT INTO conv_groups(id,name,sort,collapsed,created_at) VALUES(?1,?2,?3,0,?4)", params![g.id, g.name, g.sort, g.created_at])?;
        Ok(g)
    }

    pub fn rename_group(&self, id: &str, name: &str) -> AppResult<()> {
        let name = clean_group_name(name)?;
        if self.lock().execute("UPDATE conv_groups SET name=?1 WHERE id=?2", params![name, id])? == 0 {
            return Err(AppError::NotFound("group".into()));
        }
        Ok(())
    }

    pub fn set_group_collapsed(&self, id: &str, collapsed: bool) -> AppResult<()> {
        self.lock().execute("UPDATE conv_groups SET collapsed=?1 WHERE id=?2", params![collapsed as i64, id])?;
        Ok(())
    }

    /// Delete a group; its chats are kept and become ungrouped.
    pub fn delete_group(&self, id: &str) -> AppResult<()> {
        let mut c = self.lock();
        let tx = c.transaction()?;
        tx.execute("UPDATE conversations SET group_id=NULL WHERE group_id=?1", params![id])?;
        tx.execute("DELETE FROM conv_groups WHERE id=?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn move_conversation(&self, conv: &str, group: Option<&str>) -> AppResult<()> {
        let c = self.lock();
        if let Some(g) = group {
            let exists: Option<String> = c.query_row("SELECT id FROM conv_groups WHERE id=?1", params![g], |r| r.get(0)).optional()?;
            if exists.is_none() {
                return Err(AppError::NotFound("group".into()));
            }
        }
        c.execute("UPDATE conversations SET group_id=?1 WHERE id=?2", params![group, conv])?;
        Ok(())
    }

    pub fn list_conversations(&self) -> AppResult<Vec<ConversationSummary>> {
        let c = self.lock();
        let mut st = c.prepare("SELECT id,title,mode,workspace,created_at,updated_at,group_id,config_json FROM conversations ORDER BY updated_at DESC LIMIT 1000")?;
        let rows = st.query_map([], row_summary)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn get_conversation(&self, id: &str) -> AppResult<Conversation> {
        let c = self.lock();
        let (summary, usage_json): (ConversationSummary, Option<String>) = c
            .query_row("SELECT id,title,mode,workspace,created_at,updated_at,group_id,config_json,usage_json FROM conversations WHERE id=?1", params![id], |r| {
                Ok((row_summary(r)?, r.get(8)?))
            })
            .optional()?
            .ok_or_else(|| AppError::NotFound(format!("conversation {id}")))?;
        let mut st = c.prepare("SELECT json FROM messages WHERE conversation_id=?1 ORDER BY seq")?;
        let messages = st
            .query_map(params![id], |r| r.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .filter_map(|j| serde_json::from_str::<Message>(&j).ok()) // skip unreadable rows instead of failing the whole chat
            .collect();
        let usage = usage_json.and_then(|u| serde_json::from_str(&u).ok());
        Ok(Conversation { summary, messages, usage })
    }

    /// Append or update (same id) a message. New messages get the next seq.
    pub fn upsert_message(&self, conv_id: &str, m: &Message) -> AppResult<()> {
        let json = serde_json::to_string(m)?;
        let mut c = self.lock();
        let tx = c.transaction()?;
        let updated = tx.execute("UPDATE messages SET json=?1 WHERE id=?2 AND conversation_id=?3", params![json, m.id, conv_id])?;
        if updated == 0 {
            let seq: i64 = tx.query_row("SELECT COALESCE(MAX(seq),-1)+1 FROM messages WHERE conversation_id=?1", params![conv_id], |r| r.get(0))?;
            tx.execute("INSERT INTO messages(id,conversation_id,seq,json) VALUES(?1,?2,?3,?4)", params![m.id, conv_id, seq, json])?;
        }
        tx.execute("UPDATE conversations SET updated_at=?1 WHERE id=?2", params![now_ms(), conv_id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_message(&self, conv_id: &str, msg_id: &str) -> AppResult<()> {
        self.lock().execute("DELETE FROM messages WHERE id=?1 AND conversation_id=?2", params![msg_id, conv_id])?;
        Ok(())
    }

    pub fn set_title(&self, id: &str, title: &str) -> AppResult<()> {
        self.lock().execute("UPDATE conversations SET title=?1 WHERE id=?2", params![title, id])?;
        Ok(())
    }

    /// Save a chat's own model settings (mode is kept in sync with the `mode` column).
    pub fn set_conversation_config(&self, id: &str, cfg: &crate::model::ConvConfig) -> AppResult<()> {
        let json = serde_json::to_string(cfg)?;
        let n = self.lock().execute("UPDATE conversations SET config_json=?1, mode=?2 WHERE id=?3", params![json, mode_str(cfg.mode), id])?;
        if n == 0 {
            return Err(AppError::NotFound(format!("conversation {id}")));
        }
        Ok(())
    }

    pub fn set_conversation_meta(&self, id: &str, mode: Mode, workspace: Option<&str>) -> AppResult<()> {
        self.lock().execute("UPDATE conversations SET mode=?1, workspace=?2 WHERE id=?3", params![mode_str(mode), workspace, id])?;
        Ok(())
    }

    pub fn add_usage(&self, id: &str, u: &Usage) -> AppResult<Usage> {
        let c = self.lock();
        let cur: Option<String> = c.query_row("SELECT usage_json FROM conversations WHERE id=?1", params![id], |r| r.get(0)).optional()?.flatten();
        let mut total: Usage = cur.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        total.add(u);
        c.execute("UPDATE conversations SET usage_json=?1 WHERE id=?2", params![serde_json::to_string(&total)?, id])?;
        Ok(total)
    }

    pub fn delete_conversation(&self, id: &str) -> AppResult<()> {
        let mut c = self.lock();
        let tx = c.transaction()?;
        tx.execute("DELETE FROM messages WHERE conversation_id=?1", params![id])?;
        tx.execute("DELETE FROM conversations WHERE id=?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn schema_version(&self) -> i64 {
        self.lock().query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap_or(0)
    }
}

fn migrate(conn: &Connection) -> AppResult<()> {
    let v: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if v > SCHEMA_VERSION {
        // Downgrade: never touch a newer database.
        return Err(AppError::Db(format!("database version {v} is newer than this app ({SCHEMA_VERSION})")));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(v as usize) {
        conn.execute_batch(&format!("BEGIN; {sql} PRAGMA user_version = {}; COMMIT;", i + 1))?;
    }
    Ok(())
}

fn mode_str(m: Mode) -> &'static str {
    match m {
        Mode::Office => "office",
        Mode::Coding => "coding",
    }
}

fn row_summary(r: &rusqlite::Row) -> rusqlite::Result<ConversationSummary> {
    let mode: String = r.get(2)?;
    Ok(ConversationSummary {
        id: r.get(0)?,
        title: r.get(1)?,
        mode: if mode == "coding" { Mode::Coding } else { Mode::Office },
        workspace: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
        group_id: r.get(6)?,
        config: r.get::<_, Option<String>>(7)?.and_then(|j| serde_json::from_str(&j).ok()),
    })
}

fn clean_group_name(name: &str) -> AppResult<String> {
    let n: String = name.trim().chars().filter(|c| !c.is_control()).collect();
    if n.is_empty() || n.chars().count() > 60 {
        return Err(AppError::Invalid("group name must be 1-60 characters".into()));
    }
    Ok(n)
}

pub fn data_dir_file(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Role;

    #[test]
    fn roundtrip_conversation_and_messages() {
        let db = Db::open_in_memory().unwrap();
        db.create_conversation("c1", Mode::Coding, Some("C:/w")).unwrap();
        let mut m = Message::new(Role::User);
        m.text = "hi".into();
        db.upsert_message("c1", &m).unwrap();
        let mut a = Message::new(Role::Assistant);
        a.text = "par".into();
        db.upsert_message("c1", &a).unwrap();
        a.text = "partial done".into();
        db.upsert_message("c1", &a).unwrap(); // update in place keeps order
        db.set_title("c1", "Greeting").unwrap();
        let c = db.get_conversation("c1").unwrap();
        assert_eq!(c.messages.len(), 2);
        assert_eq!(c.messages[1].text, "partial done");
        assert_eq!(c.summary.title, "Greeting");
        assert_eq!(db.list_conversations().unwrap().len(), 1);
        db.delete_conversation("c1").unwrap();
        assert!(db.list_conversations().unwrap().is_empty());
    }

    #[test]
    fn settings_roundtrip_and_defaults_for_missing_fields() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(db.load_settings().theme, "system");
        let s = Settings { language: Some("zh-CN".into()), ..Default::default() };
        db.save_settings(&s).unwrap();
        assert_eq!(db.load_settings().language.as_deref(), Some("zh-CN"));
        // Older settings JSON with fewer fields still loads (forward migration).
        db.lock().execute("UPDATE kv SET value='{\"theme\":\"dark\"}' WHERE key='settings'", []).unwrap();
        let s2 = db.load_settings();
        assert_eq!(s2.theme, "dark");
        assert_eq!(s2.max_steps, 40);
    }

    #[test]
    fn empty_update_repo_defaults_to_official() {
        let db = Db::open_in_memory().unwrap();
        // Settings written by 0.1.0 (before the default existed) stored an empty repo.
        db.lock().execute("INSERT INTO kv(key,value) VALUES('settings','{\"update_repo\":\"\",\"theme\":\"dark\"}')", []).unwrap();
        let s = db.load_settings();
        assert_eq!(s.update_repo, DEFAULT_UPDATE_REPO);
        assert_eq!(s.theme, "dark", "other values untouched");
        for (input, want) in [("   ", DEFAULT_UPDATE_REPO), (" /me/fork/ ", "me/fork"), ("me/fork", "me/fork")] {
            let mut s = Settings { update_repo: input.into(), ..Default::default() };
            s.normalize();
            assert_eq!(s.update_repo, want, "input {input:?}");
        }
    }

    #[test]
    fn auto_update_repo_and_profile() {
        let db = Db::open_in_memory().unwrap();
        // Older settings without the new fields get the defaults.
        db.lock().execute("INSERT INTO kv(key,value) VALUES('settings','{\"update_repo\":\"me/fork\"}')", []).unwrap();
        let mut s = db.load_settings();
        assert!(s.auto_update_repo);
        assert_eq!(s.effective_update_repo(), DEFAULT_UPDATE_REPO, "auto on ignores the custom repo");
        s.auto_update_repo = false;
        assert_eq!(s.effective_update_repo(), "me/fork");
        s.profile_name = "  Ar0ng\u{7}  ".into();
        s.normalize();
        assert_eq!(s.profile_name, "Ar0ng");
        assert_eq!(db.load_avatar(), None);
        db.save_avatar(Some("data:image/png;base64,AAAA")).unwrap();
        assert_eq!(db.load_avatar().as_deref(), Some("data:image/png;base64,AAAA"));
        db.save_avatar(None).unwrap();
        assert_eq!(db.load_avatar(), None);
    }

    #[test]
    fn corrupt_settings_fall_back() {
        let db = Db::open_in_memory().unwrap();
        db.lock().execute("INSERT INTO kv(key,value) VALUES('settings','{not json')", []).unwrap();
        assert_eq!(db.load_settings().theme, "system");
        assert!(db.recovery_notice.lock().unwrap().is_some());
    }

    #[test]
    fn groups_create_move_delete() {
        let db = Db::open_in_memory().unwrap();
        let g = db.create_group("  Work  ").unwrap();
        assert_eq!(g.name, "Work");
        assert!(db.create_group("   ").is_err());
        db.create_conversation_in("c1", Mode::Office, None, Some(&g.id)).unwrap();
        db.create_conversation_in("c2", Mode::Office, None, Some("missing")).unwrap();
        db.create_conversation("c3", Mode::Office, None).unwrap();
        let by_id = |id: &str| db.list_conversations().unwrap().into_iter().find(|c| c.id == id).unwrap();
        assert_eq!(by_id("c1").group_id.as_deref(), Some(g.id.as_str()));
        assert_eq!(by_id("c2").group_id, None, "unknown group ignored");
        db.move_conversation("c3", Some(&g.id)).unwrap();
        assert!(db.move_conversation("c3", Some("nope")).is_err());
        db.rename_group(&g.id, "Projects").unwrap();
        db.set_group_collapsed(&g.id, true).unwrap();
        let gs = db.list_groups().unwrap();
        assert_eq!((gs[0].name.as_str(), gs[0].collapsed), ("Projects", true));
        db.delete_group(&g.id).unwrap();
        assert!(db.list_groups().unwrap().is_empty());
        assert_eq!(by_id("c1").group_id, None, "chats survive group deletion");
        assert_eq!(db.get_conversation("c3").unwrap().summary.group_id, None);
    }

    #[test]
    fn conversation_config_roundtrip_and_v2_upgrade() {
        use crate::model::{ConvConfig, Effort, ProviderId};
        let dir = std::env::temp_dir().join(format!("ycode-mig3-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("ycode.db");
        {
            // A v2 database (0.2.x) with one chat.
            let c = Connection::open(&p).unwrap();
            c.execute_batch(&format!("BEGIN; {} {} PRAGMA user_version = 2; COMMIT;", MIGRATIONS[0], MIGRATIONS[1])).unwrap();
            c.execute("INSERT INTO conversations(id,title,mode,workspace,created_at,updated_at) VALUES('old','Old','coding',NULL,1,1)", []).unwrap();
        }
        let db = Db::open(&p).unwrap();
        assert_eq!(db.schema_version(), 3);
        let s = db.get_conversation("old").unwrap().summary;
        assert_eq!((s.mode, s.config), (Mode::Coding, None), "old chats keep their mode, no config yet");
        let cfg = ConvConfig { provider: ProviderId::Deepseek, model: "deepseek-v4-pro".into(), effort: Some(Effort::Max), mode: Mode::Coding };
        db.set_conversation_config("old", &cfg).unwrap();
        let listed = db.list_conversations().unwrap().into_iter().find(|c| c.id == "old").unwrap();
        assert_eq!(listed.config, Some(cfg.clone()));
        assert!(db.set_conversation_config("missing", &cfg).is_err());
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn migrates_v1_database_to_v2_keeping_chats() {
        let dir = std::env::temp_dir().join(format!("ycode-mig-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("ycode.db");
        {
            // Build a database exactly as version 0.1.x (schema v1) left it.
            let c = Connection::open(&p).unwrap();
            c.execute_batch(&format!("BEGIN; {} PRAGMA user_version = 1; COMMIT;", MIGRATIONS[0])).unwrap();
            c.execute("INSERT INTO conversations(id,title,mode,workspace,created_at,updated_at) VALUES('old','Old chat','office',NULL,1,1)", []).unwrap();
            c.execute("INSERT INTO messages(id,conversation_id,seq,json) VALUES('m1','old',0,?1)", params![serde_json::to_string(&Message::new(Role::User)).unwrap()]).unwrap();
        }
        let db = Db::open(&p).unwrap();
        assert_eq!(db.schema_version(), SCHEMA_VERSION);
        assert!(db.recovery_notice.lock().unwrap().is_none());
        let c = db.get_conversation("old").unwrap();
        assert_eq!((c.summary.title.as_str(), c.messages.len(), c.summary.group_id.clone()), ("Old chat", 1, None));
        let g = db.create_group("New").unwrap();
        db.move_conversation("old", Some(&g.id)).unwrap();
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_db_file_is_recovered() {
        let dir = std::env::temp_dir().join(format!("ycode-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("ycode.db");
        std::fs::write(&p, b"this is not a sqlite database at all, just garbage bytes................").unwrap();
        let db = Db::open(&p).unwrap();
        assert!(db.recovery_notice.lock().unwrap().is_some());
        assert_eq!(db.schema_version(), SCHEMA_VERSION);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reopen_preserves_data_and_version() {
        let dir = std::env::temp_dir().join(format!("ycode-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("ycode.db");
        {
            let db = Db::open(&p).unwrap();
            db.create_conversation("x", Mode::Office, None).unwrap();
        }
        let db = Db::open(&p).unwrap();
        assert_eq!(db.list_conversations().unwrap().len(), 1);
        assert!(db.recovery_notice.lock().unwrap().is_none());
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
