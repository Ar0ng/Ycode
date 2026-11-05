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

pub const SCHEMA_VERSION: i64 = 1;

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
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub mode: Mode,
    pub workspace: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
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
    pub check_updates_on_start: bool,
    pub claude_fallbacks: bool,
    pub sidebar_collapsed: bool,
    /// Max agent steps (model calls) per user message.
    pub max_steps: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: None,
            theme: "system".into(),
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
            check_updates_on_start: true,
            claude_fallbacks: true,
            sidebar_collapsed: false,
            max_steps: 40,
        }
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
        match raw {
            None => Settings::default(),
            Some(s) => serde_json::from_str(&s).unwrap_or_else(|e| {
                // Corrupt settings: fall back to defaults but keep a copy.
                let _ = c.execute("INSERT OR REPLACE INTO kv(key,value) VALUES('settings_corrupt',?1)", params![s]);
                *self.recovery_notice.lock().unwrap_or_else(|p| p.into_inner()) = Some(format!("settings were reset ({e})"));
                Settings::default()
            }),
        }
    }

    pub fn save_settings(&self, s: &Settings) -> AppResult<()> {
        let json = serde_json::to_string(s)?;
        self.lock().execute("INSERT OR REPLACE INTO kv(key,value) VALUES('settings',?1)", params![json])?;
        Ok(())
    }

    // ---------- conversations ----------

    pub fn create_conversation(&self, id: &str, mode: Mode, workspace: Option<&str>) -> AppResult<ConversationSummary> {
        let now = now_ms();
        self.lock().execute(
            "INSERT INTO conversations(id,title,mode,workspace,created_at,updated_at) VALUES(?1,'',?2,?3,?4,?4)",
            params![id, mode_str(mode), workspace, now],
        )?;
        Ok(ConversationSummary { id: id.into(), title: String::new(), mode, workspace: workspace.map(String::from), created_at: now, updated_at: now })
    }

    pub fn list_conversations(&self) -> AppResult<Vec<ConversationSummary>> {
        let c = self.lock();
        let mut st = c.prepare("SELECT id,title,mode,workspace,created_at,updated_at FROM conversations ORDER BY updated_at DESC LIMIT 500")?;
        let rows = st.query_map([], row_summary)?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn get_conversation(&self, id: &str) -> AppResult<Conversation> {
        let c = self.lock();
        let (summary, usage_json): (ConversationSummary, Option<String>) = c
            .query_row("SELECT id,title,mode,workspace,created_at,updated_at,usage_json FROM conversations WHERE id=?1", params![id], |r| {
                Ok((row_summary(r)?, r.get(6)?))
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
    })
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
    fn corrupt_settings_fall_back() {
        let db = Db::open_in_memory().unwrap();
        db.lock().execute("INSERT INTO kv(key,value) VALUES('settings','{not json')", []).unwrap();
        assert_eq!(db.load_settings().theme, "system");
        assert!(db.recovery_notice.lock().unwrap().is_some());
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
