//! Local tools exposed to the model. Definitions are static and returned in a fixed
//! order so the tool block of every request is byte-identical (cache-stable).
//!
//! Execution is two-phase: `prepare` validates arguments, resolves paths and
//! computes previews (diffs); the agent then asks the user when `approval` is set,
//! and only afterwards calls `execute`.

pub mod sandbox;
pub mod shell;

use crate::error::{AppError, AppResult};
use crate::model::Mode;
use crate::providers::{Cancel, ToolSpec};
use crate::skills::SkillStore;
use sandbox::Resolved;
use serde::Serialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

pub const MAX_OUTPUT_CHARS: usize = 30_000;
const MAX_READ_BYTES: u64 = 5 * 1024 * 1024;
const MAX_SEARCH_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_SEARCH_FILES: usize = 20_000;

/// Tool list without image tools (kept for callers that never offer them).
pub fn specs(mode: Mode) -> Vec<ToolSpec> {
    specs_with(mode, false)
}

/// Fixed, deterministic tool list. `images` adds GPT Image tools (needs an OpenAI key).
pub fn specs_with(mode: Mode, images: bool) -> Vec<ToolSpec> {
    let mut v = vec![
        ToolSpec {
            name: "read_file",
            description: "Read a UTF-8 text file. Paths are relative to the workspace folder. Use start_line/max_lines to read a range of a large file. Returns lines prefixed with line numbers.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path, relative to the workspace" },
                    "start_line": { "type": "integer", "description": "1-based first line (default 1)" },
                    "max_lines": { "type": "integer", "description": "Maximum lines to return (default 2000)" }
                },
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: "list_dir",
            description: "List files and folders in a workspace directory. depth 1-3 (default 1). Skips .git, node_modules and build output folders when recursing.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory, relative to the workspace (default \".\")" },
                    "depth": { "type": "integer", "description": "Recursion depth 1-3" }
                }
            }),
        },
        ToolSpec {
            name: "search_files",
            description: "Search text in workspace files (respects .gitignore). Returns matching lines as path:line: text. Use a regular expression or set literal=true.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Regular expression (or literal text with literal=true)" },
                    "path": { "type": "string", "description": "Sub-folder to search (default workspace root)" },
                    "glob": { "type": "string", "description": "Optional filename glob such as *.rs" },
                    "literal": { "type": "boolean" },
                    "case_sensitive": { "type": "boolean" }
                },
                "required": ["pattern"]
            }),
        },
        ToolSpec {
            name: "write_file",
            description: "Create a new file or replace an existing file's entire content. The user reviews a diff before it is applied. Prefer edit_file for small changes to existing files.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "content": { "type": "string", "description": "Complete new file content" }
                },
                "required": ["path", "content"]
            }),
        },
    ];
    if mode == Mode::Coding {
        v.push(ToolSpec {
            name: "edit_file",
            description: "Replace an exact text snippet in an existing file. old_string must match exactly once (including whitespace) unless replace_all is true. The user reviews a diff before it is applied.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "old_string": { "type": "string" },
                    "new_string": { "type": "string" },
                    "replace_all": { "type": "boolean" }
                },
                "required": ["path", "old_string", "new_string"]
            }),
        });
        v.push(ToolSpec {
            name: "run_command",
            description: "Run a PowerShell command in the workspace folder after the user approves it. Use for builds, tests, git and other project commands. Non-interactive; output is truncated to the last 30000 characters.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" },
                    "timeout_seconds": { "type": "integer", "description": "Default 120, max 600" }
                },
                "required": ["command"]
            }),
        });
    }
    if images {
        v.push(ToolSpec {
            name: "generate_image",
            description: "Create a new image with OpenAI GPT Image from a text description. Use only when the user asks for an image (picture, illustration, logo, icon, poster…). Write a detailed prompt in English or the user's language. The result is shown to the user automatically.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "Detailed description of the image" },
                    "size": { "type": "string", "enum": crate::images::SIZES, "description": "auto, square 1024x1024, landscape 1536x1024 or portrait 1024x1536" },
                    "quality": { "type": "string", "enum": crate::images::QUALITIES },
                    "transparent_background": { "type": "boolean" }
                },
                "required": ["prompt"]
            }),
        });
        v.push(ToolSpec {
            name: "edit_image",
            description: "Edit an existing image with OpenAI GPT Image. `image` is the id of an image generated earlier in this chat (img-….png) or a PNG/JPEG/WebP path inside the workspace. Describe the change in `prompt`.",
            parameters: json!({
                "type": "object",
                "properties": {
                    "image": { "type": "string" },
                    "prompt": { "type": "string", "description": "What to change" },
                    "size": { "type": "string", "enum": crate::images::SIZES },
                    "quality": { "type": "string", "enum": crate::images::QUALITIES }
                },
                "required": ["image", "prompt"]
            }),
        });
    }
    v.push(ToolSpec {
        name: "load_skill",
        description: "Load the full instructions of an installed skill by name (see the skills list in the system prompt). Call this before following a skill.",
        parameters: json!({
            "type": "object",
            "properties": { "name": { "type": "string" } },
            "required": ["name"]
        }),
    });
    v.push(ToolSpec {
        name: "read_skill_file",
        description: "Read a reference or asset text file that belongs to a skill, e.g. references/guide.md. Only files inside that skill's folder are readable.",
        parameters: json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Skill name" },
                "path": { "type": "string", "description": "Path relative to the skill folder" }
            },
            "required": ["name", "path"]
        }),
    });
    v
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalKind {
    WriteFile,
    RunCommand,
    ReadOutside,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApprovalRequest {
    pub kind: ApprovalKind,
    /// Short human-readable target (path or command).
    pub target: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    /// For commands: why it is considered risky (empty if routine).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

pub enum Action {
    Read { path: PathBuf, start: usize, max: usize },
    List { path: PathBuf, depth: usize },
    Search { root: PathBuf, re: regex::Regex, glob: Option<String>, display_root: PathBuf },
    Write { path: PathBuf, content: String, diff: String },
    Command { command: String, cwd: PathBuf, timeout: u64 },
    Image(crate::images::ImageJob),
    Text(String),
}

pub struct Prepared {
    pub action: Action,
    pub approval: Option<ApprovalRequest>,
}

pub struct ToolCtx<'a> {
    pub mode: Mode,
    pub workspace: Option<&'a Path>,
    pub skills: &'a SkillStore,
    /// Present when image tools are enabled and an OpenAI key exists.
    pub images: Option<crate::images::ImageCtx>,
}

fn arg_str<'a>(args: &'a Value, k: &str) -> AppResult<&'a str> {
    args.get(k).and_then(|v| v.as_str()).ok_or_else(|| AppError::Invalid(format!("missing string argument '{k}'")))
}

fn arg_u64(args: &Value, k: &str) -> Option<u64> {
    args.get(k).and_then(|v| v.as_u64().or_else(|| v.as_f64().map(|f| f.max(0.0) as u64)))
}

fn arg_bool(args: &Value, k: &str) -> bool {
    args.get(k).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn require_ws<'a>(ctx: &'a ToolCtx) -> AppResult<&'a Path> {
    ctx.workspace.ok_or_else(|| AppError::Permission("no workspace folder is authorised; ask the user to choose one with the folder button".into()))
}

pub fn prepare(name: &str, args: &Value, ctx: &ToolCtx) -> AppResult<Prepared> {
    if let Some(raw) = args.get("_raw") {
        return Err(AppError::Invalid(format!("arguments were not valid JSON: {}", crate::error::truncate(raw.as_str().unwrap_or(""), 200))));
    }
    let allowed: Vec<&str> = specs_with(ctx.mode, ctx.images.is_some()).iter().map(|s| s.name).collect();
    if !allowed.contains(&name) {
        return Err(AppError::Permission(format!("tool '{name}' is not available in {:?} mode", ctx.mode)));
    }
    match name {
        "read_file" => {
            let r = sandbox::resolve(ctx.workspace, arg_str(args, "path")?)?;
            let start = arg_u64(args, "start_line").unwrap_or(1).max(1) as usize;
            let max = arg_u64(args, "max_lines").unwrap_or(2000).clamp(1, 5000) as usize;
            let approval = outside_read(&r);
            Ok(Prepared { action: Action::Read { path: r.path().to_path_buf(), start, max }, approval })
        }
        "list_dir" => {
            let p = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let r = sandbox::resolve(ctx.workspace, p)?;
            let depth = arg_u64(args, "depth").unwrap_or(1).clamp(1, 3) as usize;
            let approval = outside_read(&r);
            Ok(Prepared { action: Action::List { path: r.path().to_path_buf(), depth }, approval })
        }
        "search_files" => {
            let ws = require_ws(ctx)?;
            let p = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            let r = sandbox::resolve(Some(ws), p)?;
            let Resolved::Inside(root) = r else {
                return Err(AppError::Permission("search is limited to the workspace folder".into()));
            };
            let pat = arg_str(args, "pattern")?;
            let pat = if arg_bool(args, "literal") { regex::escape(pat) } else { pat.to_string() };
            let re = regex::RegexBuilder::new(&pat)
                .case_insensitive(!arg_bool(args, "case_sensitive"))
                .size_limit(1 << 20)
                .build()
                .map_err(|e| AppError::Invalid(format!("invalid regex: {e}")))?;
            let glob = args.get("glob").and_then(|v| v.as_str()).map(String::from);
            Ok(Prepared { action: Action::Search { root, re, glob, display_root: sandbox::dunce_canonicalize(ws)? }, approval: None })
        }
        "write_file" => {
            let path = writable(ctx, arg_str(args, "path")?)?;
            let content = arg_str(args, "content")?.to_string();
            let old = read_existing_text(&path)?;
            let diff = unified_diff(&path, old.as_deref().unwrap_or(""), &content);
            let approval = Some(ApprovalRequest { kind: ApprovalKind::WriteFile, target: path.display().to_string(), diff: Some(diff.clone()), warning: None });
            Ok(Prepared { action: Action::Write { path, content, diff }, approval })
        }
        "edit_file" => {
            let path = writable(ctx, arg_str(args, "path")?)?;
            let old_s = arg_str(args, "old_string")?;
            let new_s = arg_str(args, "new_string")?;
            if old_s.is_empty() {
                return Err(AppError::Invalid("old_string must not be empty; use write_file to create files".into()));
            }
            let old = read_existing_text(&path)?.ok_or_else(|| AppError::NotFound(format!("{} does not exist", path.display())))?;
            // Normalise CRLF so models that emit LF can still match Windows files.
            let crlf = old.contains("\r\n");
            let (o, n) = if crlf && !old_s.contains("\r\n") { (old_s.replace('\n', "\r\n"), new_s.replace('\n', "\r\n")) } else { (old_s.to_string(), new_s.to_string()) };
            let count = old.matches(&o).count();
            let content = match (count, arg_bool(args, "replace_all")) {
                (0, _) => return Err(AppError::Invalid("old_string was not found in the file; read the file again and copy the exact text".into())),
                (1, _) | (_, true) => old.replace(&o, &n),
                (c, false) => return Err(AppError::Invalid(format!("old_string matches {c} places; add surrounding context or set replace_all"))),
            };
            let diff = unified_diff(&path, &old, &content);
            let approval = Some(ApprovalRequest { kind: ApprovalKind::WriteFile, target: path.display().to_string(), diff: Some(diff.clone()), warning: None });
            Ok(Prepared { action: Action::Write { path, content, diff }, approval })
        }
        "run_command" => {
            let ws = require_ws(ctx)?;
            let command = arg_str(args, "command")?.trim().to_string();
            if command.is_empty() {
                return Err(AppError::Invalid("empty command".into()));
            }
            if let Some(reason) = shell::blocked(&command) {
                return Err(AppError::Permission(format!("command blocked by Ycode safety policy: {reason}")));
            }
            let timeout = arg_u64(args, "timeout_seconds").unwrap_or(120).clamp(5, 600);
            let warning = shell::risk(&command).map(String::from);
            Ok(Prepared {
                action: Action::Command { command: command.clone(), cwd: ws.to_path_buf(), timeout },
                approval: Some(ApprovalRequest { kind: ApprovalKind::RunCommand, target: command, diff: None, warning }),
            })
        }
        "generate_image" | "edit_image" => {
            let ictx = ctx.images.clone().ok_or_else(|| AppError::Permission("image generation is turned off or no OpenAI key is saved".into()))?;
            let prompt = arg_str(args, "prompt")?.trim().to_string();
            if prompt.is_empty() || prompt.chars().count() > 32_000 {
                return Err(AppError::Invalid("prompt must be 1-32000 characters".into()));
            }
            let pick = |k: &str, allowed: &[&str]| -> AppResult<String> {
                match args.get(k).and_then(|v| v.as_str()) {
                    None => Ok("auto".into()),
                    Some(v) if allowed.contains(&v) => Ok(v.into()),
                    Some(v) => Err(AppError::Invalid(format!("{k} must be one of {allowed:?}, got '{v}'"))),
                }
            };
            let size = pick("size", &crate::images::SIZES)?;
            let quality = pick("quality", &crate::images::QUALITIES)?;
            let kind = if name == "edit_image" {
                let img = arg_str(args, "image")?.trim();
                let source = if crate::images::is_image_id(img) {
                    crate::images::path_for(&ictx.dir, img)?
                } else {
                    match sandbox::resolve(ctx.workspace, img)? {
                        Resolved::Inside(p) => p,
                        Resolved::Outside(p) => return Err(AppError::Permission(format!("{} is outside the workspace", p.display()))),
                    }
                };
                crate::images::check_source_size(&source)?;
                crate::images::ImageKind::Edit { source }
            } else {
                crate::images::ImageKind::Generate
            };
            let background = arg_bool(args, "transparent_background").then(|| "transparent".to_string());
            Ok(Prepared { action: Action::Image(crate::images::ImageJob { kind, prompt, size, quality, background, ctx: ictx }), approval: None })
        }
        "load_skill" => {
            let n = arg_str(args, "name")?;
            Ok(Prepared { action: Action::Text(ctx.skills.load_instructions(n)?), approval: None })
        }
        "read_skill_file" => {
            let t = ctx.skills.read_resource(arg_str(args, "name")?, arg_str(args, "path")?)?;
            Ok(Prepared { action: Action::Text(t), approval: None })
        }
        other => Err(AppError::Invalid(format!("unknown tool '{other}'"))),
    }
}

fn outside_read(r: &Resolved) -> Option<ApprovalRequest> {
    match r {
        Resolved::Inside(_) => None,
        Resolved::Outside(p) => Some(ApprovalRequest { kind: ApprovalKind::ReadOutside, target: p.display().to_string(), diff: None, warning: None }),
    }
}

fn writable(ctx: &ToolCtx, p: &str) -> AppResult<PathBuf> {
    let ws = require_ws(ctx)?;
    match sandbox::resolve(Some(ws), p)? {
        Resolved::Inside(path) => {
            if let Some(why) = sandbox::write_forbidden(&path) {
                return Err(AppError::Permission(why.into()));
            }
            if path.is_dir() {
                return Err(AppError::Invalid(format!("{} is a directory", path.display())));
            }
            Ok(path)
        }
        Resolved::Outside(path) => Err(AppError::Permission(format!("{} is outside the workspace; writing there is not allowed", path.display()))),
    }
}

fn read_existing_text(p: &Path) -> AppResult<Option<String>> {
    match std::fs::metadata(p) {
        Err(_) => Ok(None),
        Ok(m) if m.len() > MAX_READ_BYTES => Err(AppError::Invalid("file is larger than 5 MB".into())),
        Ok(_) => {
            let bytes = std::fs::read(p)?;
            if bytes.contains(&0) {
                return Err(AppError::Invalid("file looks binary".into()));
            }
            Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
        }
    }
}

pub fn unified_diff(path: &Path, old: &str, new: &str) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    similar::TextDiff::from_lines(old, new).unified_diff().context_radius(3).header(&format!("a/{name}"), &format!("b/{name}")).to_string()
}

/// Run an action; also returns ids of images it produced.
pub async fn execute_full(action: Action, cancel: &Cancel) -> AppResult<(String, Vec<String>)> {
    match action {
        Action::Image(job) => crate::images::run(job, cancel).await,
        other => execute(other, cancel).await.map(|t| (t, vec![])),
    }
}

/// Run a prepared action. Errors become tool results (is_error) at the agent level.
pub async fn execute(action: Action, cancel: &Cancel) -> AppResult<String> {
    match action {
        Action::Image(job) => crate::images::run(job, cancel).await.map(|(t, _)| t),
        Action::Text(t) => Ok(cap(t)),
        Action::Read { path, start, max } => {
            let meta = std::fs::metadata(&path).map_err(|e| AppError::Io(format!("{}: {e}", path.display())))?;
            if meta.is_dir() {
                return Err(AppError::Invalid(format!("{} is a directory; use list_dir", path.display())));
            }
            if meta.len() > MAX_READ_BYTES {
                return Err(AppError::Invalid("file is larger than 5 MB".into()));
            }
            let bytes = tokio::fs::read(&path).await?;
            if bytes.iter().take(8192).any(|b| *b == 0) {
                return Err(AppError::Invalid("file looks binary; only text files can be read".into()));
            }
            let text = String::from_utf8_lossy(&bytes);
            let total = text.lines().count();
            let mut out = String::new();
            for (i, line) in text.lines().enumerate().skip(start - 1).take(max) {
                out.push_str(&format!("{:>6}\t{}\n", i + 1, line));
            }
            let shown_end = (start - 1 + max).min(total);
            if total > shown_end || start > 1 {
                out.push_str(&format!("\n[lines {start}-{shown_end} of {total}]"));
            }
            if out.is_empty() {
                out = "(empty file)".into();
            }
            Ok(cap(out))
        }
        Action::List { path, depth } => {
            let mut out = Vec::new();
            list_rec(&path, &path, depth, &mut out)?;
            if out.len() >= 500 {
                out.truncate(500);
                out.push("… (truncated at 500 entries)".into());
            }
            Ok(if out.is_empty() { "(empty directory)".into() } else { out.join("\n") })
        }
        Action::Search { root, re, glob, display_root } => {
            let cancel = cancel.clone();
            tokio::task::spawn_blocking(move || search(&root, &re, glob.as_deref(), &display_root, &cancel))
                .await
                .map_err(|e| AppError::Io(e.to_string()))?
        }
        Action::Write { path, content, .. } => {
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            // Write to a temp file then rename so a crash never leaves a half file.
            let tmp = path.with_extension(format!("ycode-tmp-{}", std::process::id()));
            tokio::fs::write(&tmp, content.as_bytes()).await?;
            tokio::fs::rename(&tmp, &path).await.inspect_err(|_| {
                let _ = std::fs::remove_file(&tmp);
            })?;
            Ok(format!("Wrote {} ({} bytes).", path.display(), content.len()))
        }
        Action::Command { command, cwd, timeout } => shell::run(&command, &cwd, timeout, cancel).await,
    }
}

const SKIP_DIRS: [&str; 6] = [".git", "node_modules", "target", "dist", ".venv", "__pycache__"];

fn list_rec(root: &Path, dir: &Path, depth: usize, out: &mut Vec<String>) -> AppResult<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir).map_err(|e| AppError::Io(format!("{}: {e}", dir.display())))?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        if out.len() >= 500 {
            return Ok(());
        }
        let p = e.path();
        let rel = p.strip_prefix(root).unwrap_or(&p).to_string_lossy().replace('\\', "/");
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if is_dir {
            out.push(format!("{rel}/"));
            let name = e.file_name().to_string_lossy().to_string();
            if depth > 1 && !SKIP_DIRS.contains(&name.as_str()) {
                list_rec(root, &p, depth - 1, out)?;
            }
        } else {
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            out.push(format!("{rel}  ({size} B)"));
        }
    }
    Ok(())
}

fn search(root: &Path, re: &regex::Regex, glob: Option<&str>, display_root: &Path, cancel: &Cancel) -> AppResult<String> {
    let mut wb = ignore::WalkBuilder::new(root);
    wb.hidden(false).git_ignore(true).git_exclude(true).require_git(false).parents(true).filter_entry(|e| {
        let n = e.file_name().to_string_lossy();
        !(e.file_type().is_some_and(|t| t.is_dir()) && SKIP_DIRS.contains(&n.as_ref()))
    });
    if let Some(g) = glob {
        let mut ob = ignore::overrides::OverrideBuilder::new(root);
        ob.add(g).map_err(|e| AppError::Invalid(format!("invalid glob: {e}")))?;
        wb.overrides(ob.build().map_err(|e| AppError::Invalid(e.to_string()))?);
    }
    let mut hits = Vec::new();
    let mut files = 0usize;
    for entry in wb.build().filter_map(|e| e.ok()) {
        if cancel.is_cancelled() {
            return Err(AppError::Cancelled);
        }
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        files += 1;
        if files > MAX_SEARCH_FILES {
            hits.push("… stopped after 20000 files; narrow the path or glob".into());
            break;
        }
        let p = entry.path();
        if std::fs::metadata(p).map(|m| m.len() > MAX_SEARCH_FILE_BYTES).unwrap_or(true) {
            continue;
        }
        let Ok(bytes) = std::fs::read(p) else { continue };
        if bytes.iter().take(8192).any(|b| *b == 0) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        let rel = p.strip_prefix(display_root).unwrap_or(p).to_string_lossy().replace('\\', "/");
        for (i, line) in text.lines().enumerate() {
            if re.is_match(line) {
                hits.push(format!("{rel}:{}: {}", i + 1, crate::error::truncate(line.trim_end(), 300)));
                if hits.len() >= 200 {
                    hits.push("… (200 matches shown; refine the pattern)".into());
                    return Ok(cap(hits.join("\n")));
                }
            }
        }
    }
    Ok(if hits.is_empty() { "No matches.".into() } else { cap(hits.join("\n")) })
}

pub fn cap(s: String) -> String {
    if s.len() <= MAX_OUTPUT_CHARS {
        return s;
    }
    let mut cut = MAX_OUTPUT_CHARS;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}\n… [output truncated: {} of {} bytes shown]", &s[..cut], cut, s.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws() -> PathBuf {
        let d = std::env::temp_dir().join(format!("ycode-tools-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("src")).unwrap();
        std::fs::write(d.join("src/main.rs"), "fn main() {\r\n    println!(\"hi\");\r\n}\r\n").unwrap();
        std::fs::write(d.join(".gitignore"), "ignored/\n").unwrap();
        std::fs::create_dir_all(d.join("ignored")).unwrap();
        std::fs::write(d.join("ignored/x.rs"), "println!").unwrap();
        sandbox::dunce_canonicalize(&d).unwrap()
    }

    fn ctx<'a>(w: &'a Path, s: &'a SkillStore, mode: Mode) -> ToolCtx<'a> {
        ToolCtx { mode, workspace: Some(w), skills: s, images: None }
    }

    #[test]
    fn tool_list_is_stable_and_mode_scoped() {
        let a: Vec<_> = specs(Mode::Coding).iter().map(|s| s.name).collect();
        let b: Vec<_> = specs(Mode::Coding).iter().map(|s| s.name).collect();
        assert_eq!(a, b);
        assert!(a.contains(&"run_command"));
        let o: Vec<_> = specs(Mode::Office).iter().map(|s| s.name).collect();
        assert!(!o.contains(&"run_command") && !o.contains(&"edit_file"));
        assert_eq!(serde_json::to_string(&specs(Mode::Coding)).unwrap(), serde_json::to_string(&specs(Mode::Coding)).unwrap());
    }

    #[tokio::test]
    async fn read_list_search() {
        let w = ws();
        let s = SkillStore::empty();
        let c = ctx(&w, &s, Mode::Coding);
        let p = prepare("read_file", &json!({"path":"src/main.rs"}), &c).unwrap();
        assert!(p.approval.is_none());
        let out = execute(p.action, &Cancel::default()).await.unwrap();
        assert!(out.contains("     2\t    println!"));
        let p = prepare("list_dir", &json!({"depth":2}), &c).unwrap();
        let out = execute(p.action, &Cancel::default()).await.unwrap();
        assert!(out.contains("src/main.rs"));
        let p = prepare("search_files", &json!({"pattern":"println!","literal":true}), &c).unwrap();
        let out = execute(p.action, &Cancel::default()).await.unwrap();
        assert!(out.contains("src/main.rs:2:"));
        assert!(!out.contains("ignored/x.rs"), "gitignored files must be skipped: {out}");
    }

    #[tokio::test]
    async fn edit_requires_approval_and_produces_diff() {
        let w = ws();
        let s = SkillStore::empty();
        let c = ctx(&w, &s, Mode::Coding);
        let p = prepare("edit_file", &json!({"path":"src/main.rs","old_string":"println!(\"hi\");","new_string":"println!(\"bye\");"}), &c).unwrap();
        let ap = p.approval.as_ref().unwrap();
        assert_eq!(ap.kind, ApprovalKind::WriteFile);
        assert!(ap.diff.as_ref().unwrap().contains("+    println!(\"bye\");"));
        execute(p.action, &Cancel::default()).await.unwrap();
        let after = std::fs::read_to_string(w.join("src/main.rs")).unwrap();
        assert!(after.contains("bye") && after.contains("\r\n"));
        // Ambiguous / missing matches are rejected with guidance.
        assert!(prepare("edit_file", &json!({"path":"src/main.rs","old_string":"nothing-here","new_string":"x"}), &c).is_err());
    }

    #[test]
    fn writes_outside_workspace_denied_reads_need_approval() {
        let w = ws();
        let s = SkillStore::empty();
        let c = ctx(&w, &s, Mode::Coding);
        assert!(matches!(prepare("write_file", &json!({"path":"../evil.txt","content":"x"}), &c), Err(AppError::Permission(_))));
        let p = prepare("read_file", &json!({"path":"../other.txt"}), &c).unwrap();
        assert_eq!(p.approval.unwrap().kind, ApprovalKind::ReadOutside);
        assert!(matches!(prepare("write_file", &json!({"path":".git/config","content":"x"}), &c), Err(AppError::Permission(_))));
    }

    #[test]
    fn office_mode_has_no_terminal() {
        let w = ws();
        let s = SkillStore::empty();
        let c = ctx(&w, &s, Mode::Office);
        assert!(matches!(prepare("run_command", &json!({"command":"dir"}), &c), Err(AppError::Permission(_))));
    }

    #[test]
    fn invalid_json_args_reported() {
        let w = ws();
        let s = SkillStore::empty();
        let c = ctx(&w, &s, Mode::Coding);
        let e = prepare("read_file", &json!({"_raw":"{\"path\": "}), &c).err().unwrap();
        assert!(e.to_string().contains("not valid JSON"));
        assert!(prepare("read_file", &json!({}), &c).is_err());
    }

    #[test]
    fn cap_truncates_on_char_boundary() {
        let s = "你".repeat(20_000);
        let c = cap(s);
        assert!(c.contains("output truncated"));
    }
}
