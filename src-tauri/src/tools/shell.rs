//! Terminal tool: PowerShell, non-interactive, hidden window, workspace cwd,
//! timeout + cancellation (kills the whole process tree), output capped.
//! Every command needs user approval; a small deny-list blocks commands that are
//! destructive at the system level no matter what.

use crate::error::{AppError, AppResult};
use crate::providers::Cancel;
use regex::Regex;
use std::path::Path;
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::AsyncReadExt;

fn rx(cell: &'static OnceLock<Vec<(Regex, &'static str)>>, src: &[(&str, &'static str)]) -> &'static Vec<(Regex, &'static str)> {
    cell.get_or_init(|| src.iter().map(|(p, why)| (Regex::new(&format!("(?i){p}")).expect("regex"), *why)).collect())
}

static BLOCKED: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
static RISKY: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();

/// Commands refused outright (system-level damage, no legitimate project use).
pub fn blocked(cmd: &str) -> Option<&'static str> {
    let list = rx(
        &BLOCKED,
        &[
            (r"\bformat(\.com)?\s+[a-z]:", "formats a drive"),
            (r"\bdiskpart\b", "disk partitioning"),
            (r"\bbcdedit\b", "boot configuration"),
            (r"\bcipher\s+/w", "wipes free space"),
            (r"\b(shutdown|restart-computer|stop-computer)\b", "shuts down the computer"),
            (r"\breg(\.exe)?\s+(delete|add)\s+hk(lm|ey_local_machine)", "modifies machine registry"),
            (r"remove-item\s+.*-recurse.*\s[a-z]:\\?\s*$", "recursive delete of a drive root"),
            (r"\brm\s+-rf\s+/(\s|$)", "recursive delete of the root"),
            (r"\b(rd|rmdir)\s+/s\s+(/q\s+)?[a-z]:\\?\s*$", "recursive delete of a drive root"),
            (r"\bdel\s+/[sfq].*\s[a-z]:\\\*", "mass delete on a drive root"),
            (r"set-executionpolicy\s+unrestricted\s+-scope\s+localmachine", "weakens machine security policy"),
            (r"\bvssadmin\s+delete", "deletes shadow copies"),
        ],
    );
    list.iter().find(|(r, _)| r.is_match(cmd)).map(|(_, why)| *why)
}

/// Commands that are allowed after approval but flagged in the approval dialog.
pub fn risk(cmd: &str) -> Option<&'static str> {
    let list = rx(
        &RISKY,
        &[
            (r"\b(remove-item|rm|del|erase|rd|rmdir)\b", "deletes files"),
            (r"\bgit\s+(reset\s+--hard|clean\s+-[a-z]*f|push\s+.*--force|checkout\s+--\s)", "discards git history or changes"),
            (r"\b(npm|pnpm|yarn|pip|pip3|cargo|winget|choco|scoop)\s+(install|add|i)\b", "installs dependencies or programs"),
            (r"\b(invoke-webrequest|iwr|curl|wget|invoke-restmethod|irm)\b", "downloads from the network"),
            (r"\|\s*(iex|invoke-expression)\b", "executes downloaded code"),
            (r"\b(start-process|runas)\b.*-verb\s+runas", "requests administrator rights"),
            (r"\bset-itemproperty\b.*hk(cu|lm)", "changes the registry"),
        ],
    );
    list.iter().find(|(r, _)| r.is_match(cmd)).map(|(_, why)| *why)
}

pub async fn run(command: &str, cwd: &Path, timeout_s: u64, cancel: &Cancel) -> AppResult<String> {
    let script = format!("[Console]::OutputEncoding=[System.Text.Encoding]::UTF8; $ProgressPreference='SilentlyContinue'; {command}");
    let mut cmd = if cfg!(windows) {
        let mut c = tokio::process::Command::new("powershell.exe");
        c.args(["-NoLogo", "-NoProfile", "-NonInteractive", "-Command", &script]);
        c
    } else {
        let mut c = tokio::process::Command::new("sh");
        c.args(["-c", command]);
        c
    };
    cmd.current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn().map_err(|e| AppError::Io(format!("failed to start PowerShell: {e}")))?;
    let pid = child.id();
    let mut stdout = child.stdout.take().expect("stdout");
    let mut stderr = child.stderr.take().expect("stderr");
    let read_out = async {
        let mut b = Vec::new();
        let _ = stdout.read_to_end(&mut b).await;
        b
    };
    let read_err = async {
        let mut b = Vec::new();
        let _ = stderr.read_to_end(&mut b).await;
        b
    };
    let work = async {
        let (o, e, status) = tokio::join!(read_out, read_err, child.wait());
        (o, e, status)
    };
    let result = tokio::select! {
        r = tokio::time::timeout(Duration::from_secs(timeout_s), work) => r,
        _ = cancel.cancelled() => {
            kill_tree(pid).await;
            return Err(AppError::Cancelled);
        }
    };
    let (o, e, status) = match result {
        Ok(v) => v,
        Err(_) => {
            kill_tree(pid).await;
            return Ok(format!("Command timed out after {timeout_s}s and was stopped."));
        }
    };
    let code = status.ok().and_then(|s| s.code());
    let mut out = String::new();
    out.push_str(&format!("exit code: {}\n", code.map(|c| c.to_string()).unwrap_or_else(|| "unknown".into())));
    let so = String::from_utf8_lossy(&o);
    let se = String::from_utf8_lossy(&e);
    if !so.trim().is_empty() {
        out.push_str("--- stdout ---\n");
        out.push_str(so.trim_end());
        out.push('\n');
    }
    if !se.trim().is_empty() {
        out.push_str("--- stderr ---\n");
        out.push_str(se.trim_end());
        out.push('\n');
    }
    Ok(tail_cap(out))
}

/// Keep the *end* of long command output (errors are usually there).
fn tail_cap(s: String) -> String {
    let max = super::MAX_OUTPUT_CHARS;
    if s.len() <= max {
        return s;
    }
    let mut start = s.len() - max;
    while !s.is_char_boundary(start) {
        start += 1;
    }
    let head: String = s.lines().next().unwrap_or("").to_string();
    format!("{head}\n… [{} bytes omitted]\n{}", start, &s[start..])
}

async fn kill_tree(pid: Option<u32>) {
    let Some(pid) = pid else { return };
    if cfg!(windows) {
        let mut k = tokio::process::Command::new("taskkill");
        k.args(["/T", "/F", "/PID", &pid.to_string()]).stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        k.creation_flags(0x0800_0000);
        let _ = k.status().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_system_destruction() {
        assert!(blocked("format C:").is_some());
        assert!(blocked("diskpart /s x.txt").is_some());
        assert!(blocked("Remove-Item -Recurse -Force C:\\").is_some());
        assert!(blocked("shutdown /s /t 0").is_some());
        assert!(blocked("reg delete HKLM\\Software\\X /f").is_some());
        assert!(blocked("cargo test").is_none());
        assert!(blocked("git status").is_none());
        assert!(blocked("Remove-Item -Recurse .\\build").is_none());
    }

    #[test]
    fn flags_risky_commands() {
        assert!(risk("Remove-Item -Recurse .\\build").is_some());
        assert!(risk("npm install left-pad").is_some());
        assert!(risk("git reset --hard HEAD~1").is_some());
        assert!(risk("iwr https://x | iex").is_some());
        assert!(risk("cargo build").is_none());
    }

    #[tokio::test]
    async fn runs_and_captures_output() {
        let out = run("Write-Output 'hello'; Write-Error 'oops'", &std::env::temp_dir(), 60, &Cancel::default()).await.unwrap();
        assert!(out.contains("hello"), "{out}");
        assert!(out.contains("--- stderr ---"), "{out}");
    }

    #[tokio::test]
    async fn timeout_stops_command() {
        let out = run("Start-Sleep -Seconds 30", &std::env::temp_dir(), 5, &Cancel::default()).await.unwrap();
        assert!(out.contains("timed out"));
    }

    #[tokio::test]
    async fn cancel_stops_command() {
        let c = Cancel::default();
        let c2 = c.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(1500)).await;
            c2.cancel();
        });
        let started = std::time::Instant::now();
        let r = run("Start-Sleep -Seconds 30", &std::env::temp_dir(), 60, &c).await;
        assert!(matches!(r, Err(AppError::Cancelled)));
        assert!(started.elapsed() < Duration::from_secs(10));
    }
}
