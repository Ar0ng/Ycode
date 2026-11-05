//! Tiny file logger (`<app data>/logs/ycode.log`, rotated at 1 MB). Callers only log
//! event kinds and statuses — never API keys, message text or file contents.

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static LOG: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
const MAX: u64 = 1024 * 1024;

pub fn init(dir: PathBuf) {
    let _ = std::fs::create_dir_all(&dir);
    let _ = LOG.set(Mutex::new(Some(dir.join("ycode.log"))));
}

fn write(level: &str, msg: &str) {
    let Some(m) = LOG.get() else { return };
    let guard = m.lock().unwrap_or_else(|p| p.into_inner());
    let Some(path) = guard.as_ref() else { return };
    if std::fs::metadata(path).map(|m| m.len() > MAX).unwrap_or(false) {
        let _ = std::fs::rename(path, path.with_extension("log.1"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{} [{level}] {}", crate::model::now_ms(), redact(msg));
    }
}

/// Defence in depth: mask anything that looks like a bearer/API key.
pub fn redact(s: &str) -> String {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"(sk-[A-Za-z0-9_\-]{6,}|sk-ant-[A-Za-z0-9_\-]{6,}|Bearer\s+\S+)").expect("regex"));
    re.replace_all(s, "[REDACTED]").into_owned()
}

pub fn info(msg: &str) {
    write("INFO", msg);
}
pub fn warn(msg: &str) {
    write("WARN", msg);
}

#[cfg(test)]
mod tests {
    #[test]
    fn redacts_keys() {
        assert_eq!(super::redact("key sk-abcdef123456 ok"), "key [REDACTED] ok");
        assert_eq!(super::redact("Authorization: Bearer abc.def"), "Authorization: [REDACTED]");
    }
}
