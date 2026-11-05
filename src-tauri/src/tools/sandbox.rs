//! Path confinement for agent tools. Every path the model supplies is resolved
//! lexically (no `..` escapes), then symlinks are resolved on the deepest existing
//! ancestor so a link inside the workspace can't point outside it.

use crate::error::{AppError, AppResult};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub enum Resolved {
    /// Inside the authorised workspace.
    Inside(PathBuf),
    /// Outside the workspace — only readable after explicit user approval.
    Outside(PathBuf),
}

impl Resolved {
    pub fn path(&self) -> &Path {
        match self {
            Resolved::Inside(p) | Resolved::Outside(p) => p,
        }
    }
}

/// Lexically normalise: resolve `.` and `..` without touching the filesystem.
pub fn normalize(p: &Path) -> AppResult<PathBuf> {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::Prefix(_) | Component::RootDir => out.push(c.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return Err(AppError::Permission("path escapes its root".into()));
                }
            }
            Component::Normal(s) => out.push(s),
        }
    }
    Ok(out)
}

/// Canonicalise the deepest existing ancestor and re-append the rest.
fn canonical_lenient(p: &Path) -> AppResult<PathBuf> {
    let mut existing = p.to_path_buf();
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    while !existing.exists() {
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name.to_os_string());
                existing = parent.to_path_buf();
            }
            _ => break,
        }
    }
    let mut base = dunce_canonicalize(&existing)?;
    for r in rest.into_iter().rev() {
        base.push(r);
    }
    Ok(base)
}

/// `std::fs::canonicalize` returns `\\?\C:\...` on Windows; strip the verbatim
/// prefix for display and consistent comparisons.
pub fn dunce_canonicalize(p: &Path) -> AppResult<PathBuf> {
    let c = std::fs::canonicalize(p).map_err(|e| AppError::Io(format!("{}: {e}", p.display())))?;
    let s = c.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        return Ok(PathBuf::from(format!(r"\\{rest}")));
    }
    if let Some(rest) = s.strip_prefix(r"\\?\") {
        return Ok(PathBuf::from(rest));
    }
    Ok(c)
}

fn starts_with_ci(p: &Path, root: &Path) -> bool {
    if cfg!(windows) {
        let a = p.to_string_lossy().to_lowercase().replace('/', "\\");
        let b = root.to_string_lossy().to_lowercase().replace('/', "\\");
        let b = b.trim_end_matches('\\');
        a == b || a.starts_with(&format!("{b}\\"))
    } else {
        p.starts_with(root)
    }
}

pub fn resolve(workspace: Option<&Path>, input: &str) -> AppResult<Resolved> {
    let input = input.trim();
    if input.is_empty() {
        return Err(AppError::Invalid("empty path".into()));
    }
    if input.contains('\0') {
        return Err(AppError::Invalid("invalid path".into()));
    }
    let raw = Path::new(input);
    let joined = match (raw.is_absolute(), workspace) {
        (true, _) => raw.to_path_buf(),
        (false, Some(ws)) => ws.join(raw),
        (false, None) => return Err(AppError::Permission("no workspace folder is authorised; ask the user to choose one".into())),
    };
    let normalized = normalize(&joined)?;
    let real = canonical_lenient(&normalized)?;
    if let Some(ws) = workspace {
        let ws_real = dunce_canonicalize(ws)?;
        if starts_with_ci(&real, &ws_real) {
            return Ok(Resolved::Inside(real));
        }
    }
    Ok(Resolved::Outside(real))
}

/// Paths that are never writable even inside the workspace.
pub fn write_forbidden(p: &Path) -> Option<&'static str> {
    let s = p.to_string_lossy().to_lowercase().replace('\\', "/");
    if s.contains("/.git/") || s.ends_with("/.git") {
        return Some("writing inside .git is not allowed");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("ycode-sb-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(d.join("sub/a.txt"), "x").unwrap();
        dunce_canonicalize(&d).unwrap()
    }

    #[test]
    fn relative_inside() {
        let ws = tmp();
        assert!(matches!(resolve(Some(&ws), "sub/a.txt").unwrap(), Resolved::Inside(_)));
        assert!(matches!(resolve(Some(&ws), "sub/new/file.rs").unwrap(), Resolved::Inside(_)));
        assert!(matches!(resolve(Some(&ws), "./sub/../sub/a.txt").unwrap(), Resolved::Inside(_)));
    }

    #[test]
    fn traversal_is_outside() {
        let ws = tmp();
        assert!(matches!(resolve(Some(&ws), "../../etc/passwd").unwrap(), Resolved::Outside(_)));
        assert!(matches!(resolve(Some(&ws), "sub/../../x").unwrap(), Resolved::Outside(_)));
        let abs = std::env::temp_dir().join("elsewhere.txt");
        assert!(matches!(resolve(Some(&ws), abs.to_str().unwrap()).unwrap(), Resolved::Outside(_)));
    }

    #[test]
    fn sibling_prefix_is_not_inside() {
        let ws = tmp();
        let sibling = format!("{}-evil/x.txt", ws.display());
        assert!(matches!(resolve(Some(&ws), &sibling).unwrap(), Resolved::Outside(_)));
    }

    #[test]
    fn no_workspace_relative_rejected() {
        assert!(resolve(None, "a.txt").is_err());
    }

    #[test]
    fn git_dir_not_writable() {
        assert!(write_forbidden(Path::new("C:/w/.git/config")).is_some());
        assert!(write_forbidden(Path::new("C:/w/src/main.rs")).is_none());
    }
}
