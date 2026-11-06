//! End-to-end agent tests against a local mock HTTP server that replays scripted
//! SSE streams. No real API is called; request bodies are captured so the
//! cache-relevant properties (append-only history, stable prefix) can be asserted.

use super::*;
use crate::storage::Db;
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::Mutex as StdMutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[derive(Clone)]
enum Reply {
    Sse(String),
    Status(u16, String),
    /// Send the prefix, then hang until the client disconnects.
    Hang(String),
}

struct Mock {
    port: u16,
    bodies: Arc<StdMutex<Vec<Value>>>,
}

async fn mock(replies: Vec<Reply>) -> Mock {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let bodies = Arc::new(StdMutex::new(Vec::new()));
    let queue = Arc::new(StdMutex::new(VecDeque::from(replies)));
    let b2 = bodies.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { return };
            let bodies = b2.clone();
            let queue = queue.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut tmp = [0u8; 8192];
                let (head_end, content_len) = loop {
                    let n = sock.read(&mut tmp).await.unwrap_or(0);
                    if n == 0 {
                        return;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                    if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        let head = String::from_utf8_lossy(&buf[..pos]).to_lowercase();
                        let cl = head
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0)))
                            .unwrap_or(0);
                        break (pos + 4, cl);
                    }
                };
                while buf.len() < head_end + content_len {
                    let n = sock.read(&mut tmp).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&tmp[..n]);
                }
                if let Ok(v) = serde_json::from_slice::<Value>(&buf[head_end..]) {
                    bodies.lock().unwrap().push(v);
                }
                let reply = queue.lock().unwrap().pop_front().unwrap_or(Reply::Status(500, "{}".into()));
                match reply {
                    Reply::Sse(s) => {
                        let resp = format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}", s.len(), s);
                        let _ = sock.write_all(resp.as_bytes()).await;
                    }
                    Reply::Status(code, body) => {
                        let resp = format!("HTTP/1.1 {code} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}", body.len(), body);
                        let _ = sock.write_all(resp.as_bytes()).await;
                    }
                    Reply::Hang(prefix) => {
                        let resp = format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n{:x}\r\n{}\r\n", prefix.len(), prefix);
                        let _ = sock.write_all(resp.as_bytes()).await;
                        let _ = sock.flush().await;
                        let _ = sock.read(&mut tmp).await; // wait for client to go away
                    }
                }
            });
        }
    });
    Mock { port, bodies }
}

fn state(provider: ProviderId, port: u16, ws: Option<&std::path::Path>) -> Arc<AppState> {
    let dir = std::env::temp_dir().join(format!("ycode-agent-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let st = Arc::new(AppState::new(Db::open_in_memory().unwrap(), dir));
    lock(&st.key_override).insert(provider, "test-key".into());
    st.update_settings(|s| {
        s.base_urls.insert(provider.as_str().into(), format!("http://127.0.0.1:{port}"));
        s.workspace = ws.map(|p| p.display().to_string());
    })
    .unwrap();
    st
}

type Events = Arc<StdMutex<Vec<AgentEvent>>>;

fn collector() -> (Emit, Events) {
    let ev: Events = Arc::new(StdMutex::new(Vec::new()));
    let e2 = ev.clone();
    (Arc::new(move |e| e2.lock().unwrap().push(e)), ev)
}

fn params(provider: ProviderId, model: &str, text: &str, effort: Option<Effort>, mode: Mode) -> RunParams {
    RunParams { conversation_id: "conv-1".into(), text: Some(text.into()), attachments: vec![], provider, model: model.into(), effort, mode, group_id: None }
}

fn ds_text(text: &str, hit: u64, miss: u64) -> String {
    format!(
        "data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"reasoning_content\":\"think\"}}}}]}}\n\ndata: {{\"choices\":[{{\"index\":0,\"delta\":{{\"content\":{}}},\"finish_reason\":\"stop\"}}]}}\n\ndata: {{\"choices\":[],\"usage\":{{\"prompt_tokens\":{},\"completion_tokens\":5,\"prompt_cache_hit_tokens\":{hit},\"prompt_cache_miss_tokens\":{miss}}}}}\n\ndata: [DONE]\n\n",
        serde_json::to_string(text).unwrap(),
        hit + miss
    )
}

fn ds_tool(name: &str, args: &str) -> String {
    format!(
        "data: {{\"choices\":[{{\"index\":0,\"delta\":{{\"reasoning_content\":\"need tool\"}}}}]}}\n\ndata: {{\"choices\":[{{\"index\":0,\"delta\":{{\"tool_calls\":[{{\"index\":0,\"id\":\"call_1\",\"type\":\"function\",\"function\":{{\"name\":\"{name}\",\"arguments\":{}}}}}]}},\"finish_reason\":\"tool_calls\"}}]}}\n\ndata: {{\"choices\":[],\"usage\":{{\"prompt_tokens\":100,\"completion_tokens\":5,\"prompt_cache_hit_tokens\":0,\"prompt_cache_miss_tokens\":100}}}}\n\ndata: [DONE]\n\n",
        serde_json::to_string(args).unwrap()
    )
}

fn temp_ws() -> PathBuf {
    let d = std::env::temp_dir().join(format!("ycode-ws-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("notes.txt"), "alpha\nbeta\n").unwrap();
    crate::tools::sandbox::dunce_canonicalize(&d).unwrap()
}

#[test]
fn session_approvals_are_scoped_to_workspace() {
    let a = temp_ws();
    let b = temp_ws();
    let st = state(ProviderId::Deepseek, 1, Some(&a));
    let write = ApprovalRequest { kind: ApprovalKind::WriteFile, target: "file".into(), diff: None, warning: None };
    let command = ApprovalRequest { kind: ApprovalKind::RunCommand, target: "cargo test".into(), diff: None, warning: None };
    remember(&st, "conv", Some(&a), &write);
    remember(&st, "conv", Some(&a), &command);
    assert!(session_allows(&st, "conv", Some(&a), &write));
    assert!(session_allows(&st, "conv", Some(&a), &command));
    assert!(!session_allows(&st, "conv", Some(&b), &write));
    assert!(!session_allows(&st, "conv", Some(&b), &command));
    assert!(!session_allows(&st, "other", Some(&a), &write));
}

#[tokio::test]
async fn simple_chat_persists_and_titles() {
    let m = mock(vec![Reply::Sse(ds_text("Hello there", 0, 50))]).await;
    let st = state(ProviderId::Deepseek, m.port, None);
    let (emit, ev) = collector();
    let p = params(ProviderId::Deepseek, "deepseek-flash", "Say hello\nplease", Some(Effort::High), Mode::Office);
    run(st.clone(), p, Cancel::default(), emit).await;
    let conv = st.db.get_conversation("conv-1").unwrap();
    assert_eq!(conv.summary.title, "Say hello");
    assert_eq!(conv.messages.len(), 2);
    assert_eq!(conv.messages[1].text, "Hello there");
    assert_eq!(conv.messages[1].reasoning.as_deref(), Some("think"));
    let ev = ev.lock().unwrap();
    assert!(ev.iter().any(|e| matches!(e, AgentEvent::Finished { stopped: false })));
    assert!(ev.iter().any(|e| matches!(e, AgentEvent::Usage { .. })));
    // Office mode → no terminal tool offered.
    let body = &m.bodies.lock().unwrap()[0];
    let names: Vec<_> = body["tools"].as_array().unwrap().iter().map(|t| t["function"]["name"].as_str().unwrap().to_string()).collect();
    assert!(!names.contains(&"run_command".to_string()));
    assert_eq!(body["reasoning_effort"], "high");
}

#[tokio::test]
async fn tool_loop_is_append_only() {
    let ws = temp_ws();
    let m = mock(vec![Reply::Sse(ds_tool("read_file", "{\"path\":\"notes.txt\"}")), Reply::Sse(ds_text("It says alpha, beta.", 80, 40))]).await;
    let st = state(ProviderId::Deepseek, m.port, Some(&ws));
    let (emit, ev) = collector();
    run(st.clone(), params(ProviderId::Deepseek, "deepseek-v4-pro", "What is in notes.txt?", None, Mode::Coding), Cancel::default(), emit).await;
    let bodies = m.bodies.lock().unwrap();
    assert_eq!(bodies.len(), 2);
    let a = bodies[0]["messages"].as_array().unwrap();
    let b = bodies[1]["messages"].as_array().unwrap();
    assert!(a.iter().zip(b.iter()).all(|(x, y)| x == y), "request 2 must extend request 1 unchanged");
    assert_eq!(bodies[0]["tools"], bodies[1]["tools"]);
    assert_eq!(b[2]["reasoning_content"], "need tool", "reasoning_content replayed for tool turns");
    assert!(b[3]["content"].as_str().unwrap().contains("alpha"));
    let conv = st.db.get_conversation("conv-1").unwrap();
    assert_eq!(conv.messages.len(), 4); // user, assistant(tool call), tool, assistant
    assert_eq!(conv.messages[2].tool_results[0].status, "success");
    let ev = ev.lock().unwrap();
    assert!(ev.iter().any(|e| matches!(e, AgentEvent::ToolStatus { status, .. } if status == "running")));
}

#[tokio::test]
async fn deepseek_history_stays_stable_across_separate_user_turns() {
    let m = mock(vec![Reply::Sse(ds_text("first", 0, 100)), Reply::Sse(ds_text("second", 90, 30)), Reply::Sse(ds_text("third", 120, 20))]).await;
    let st = state(ProviderId::Deepseek, m.port, None);
    let (emit, _) = collector();
    for text in ["one", "two", "three"] {
        run(st.clone(), params(ProviderId::Deepseek, "deepseek-flash", text, Some(Effort::High), Mode::Office), Cancel::default(), emit.clone()).await;
    }
    let bodies = m.bodies.lock().unwrap();
    assert_eq!(bodies.len(), 3);
    for pair in bodies.windows(2) {
        let earlier = pair[0]["messages"].as_array().unwrap();
        let later = pair[1]["messages"].as_array().unwrap();
        assert!(later.starts_with(earlier));
        assert_eq!(pair[0]["tools"], pair[1]["tools"]);
    }
    assert_eq!(bodies[2]["messages"][2]["reasoning_content"], "think");
}

#[tokio::test]
async fn write_needs_approval_and_deny_is_respected() {
    let ws = temp_ws();
    let m = mock(vec![Reply::Sse(ds_tool("write_file", "{\"path\":\"out.txt\",\"content\":\"x\"}")), Reply::Sse(ds_text("OK, not written.", 0, 10))]).await;
    let st = state(ProviderId::Deepseek, m.port, Some(&ws));
    let (emit, ev) = collector();
    let st2 = st.clone();
    let ev2 = ev.clone();
    // Approver: deny the first approval request that shows up.
    let approver = tokio::spawn(async move {
        for _ in 0..200 {
            let id = ev2.lock().unwrap().iter().find_map(|e| match e {
                AgentEvent::ToolStatus { approval: Some(a), .. } => Some(a.request_id.clone()),
                _ => None,
            });
            if let Some(id) = id {
                respond_approval(&st2, &id, Decision::Deny).unwrap();
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        false
    });
    run(st.clone(), params(ProviderId::Deepseek, "deepseek-v4-pro", "write a file", None, Mode::Coding), Cancel::default(), emit).await;
    assert!(approver.await.unwrap(), "approval was requested");
    assert!(!ws.join("out.txt").exists(), "denied write must not happen");
    let conv = st.db.get_conversation("conv-1").unwrap();
    let tr = &conv.messages[2].tool_results[0];
    assert_eq!(tr.status, "cancelled");
    assert!(tr.diff.as_ref().unwrap().contains("+x"));
}

#[tokio::test]
async fn approve_session_applies_write() {
    let ws = temp_ws();
    let m = mock(vec![Reply::Sse(ds_tool("write_file", "{\"path\":\"out.txt\",\"content\":\"hello\"}")), Reply::Sse(ds_text("Done.", 0, 10))]).await;
    let st = state(ProviderId::Deepseek, m.port, Some(&ws));
    let (emit, ev) = collector();
    let st2 = st.clone();
    let ev2 = ev.clone();
    tokio::spawn(async move {
        for _ in 0..200 {
            let id = ev2.lock().unwrap().iter().find_map(|e| match e {
                AgentEvent::ToolStatus { approval: Some(a), .. } => Some(a.request_id.clone()),
                _ => None,
            });
            if let Some(id) = id {
                let _ = respond_approval(&st2, &id, Decision::AllowSession);
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    });
    run(st.clone(), params(ProviderId::Deepseek, "deepseek-v4-pro", "write", None, Mode::Coding), Cancel::default(), emit).await;
    assert_eq!(std::fs::read_to_string(ws.join("out.txt")).unwrap(), "hello");
    assert!(lock(&st.perms).get("conv-1").unwrap().writes.contains(&ws));
}

#[tokio::test]
async fn stop_keeps_partial_output() {
    let partial = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Partial ans\"}}]}\n\n".to_string();
    let m = mock(vec![Reply::Hang(partial)]).await;
    let st = state(ProviderId::Deepseek, m.port, None);
    let (emit, ev) = collector();
    let cancel = Cancel::default();
    let c2 = cancel.clone();
    let ev2 = ev.clone();
    tokio::spawn(async move {
        for _ in 0..300 {
            if ev2.lock().unwrap().iter().any(|e| matches!(e, AgentEvent::Delta { .. })) {
                c2.cancel();
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        c2.cancel();
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), run(st.clone(), params(ProviderId::Deepseek, "deepseek-flash", "long", None, Mode::Office), cancel, emit))
        .await
        .expect("stop must end the run quickly");
    let conv = st.db.get_conversation("conv-1").unwrap();
    assert_eq!(conv.messages[1].status, "partial");
    assert_eq!(conv.messages[1].text, "Partial ans");
    assert!(ev.lock().unwrap().iter().any(|e| matches!(e, AgentEvent::Finished { stopped: true })));
}

#[tokio::test]
async fn invalid_key_is_reported_and_retry_works() {
    let m = mock(vec![
        Reply::Status(401, r#"{"error":{"message":"Authentication Fails"}}"#.into()),
        Reply::Sse(ds_text("Recovered", 0, 10)),
    ])
    .await;
    let st = state(ProviderId::Deepseek, m.port, None);
    let (emit, ev) = collector();
    run(st.clone(), params(ProviderId::Deepseek, "deepseek-flash", "hi", None, Mode::Office), Cancel::default(), emit.clone()).await;
    let err = ev.lock().unwrap().iter().find_map(|e| match e {
        AgentEvent::Error { error: AppError::Provider(p), .. } => Some(p.kind),
        _ => None,
    });
    assert_eq!(err, Some(ProviderErrorKind::InvalidKey));
    // Retry (text: None) removes the failed message and continues.
    let mut p = params(ProviderId::Deepseek, "deepseek-flash", "", None, Mode::Office);
    p.text = None;
    run(st.clone(), p, Cancel::default(), emit).await;
    let conv = st.db.get_conversation("conv-1").unwrap();
    assert_eq!(conv.messages.len(), 2);
    assert_eq!(conv.messages[1].text, "Recovered");
    // The retried request must not contain the failed assistant turn.
    let bodies = m.bodies.lock().unwrap();
    assert_eq!(bodies[1]["messages"].as_array().unwrap().len(), 2); // system + user
}

fn oa_text(text: &str, input: u64, cached: u64) -> String {
    format!(
        "event: response.output_text.delta\ndata: {{\"type\":\"response.output_text.delta\",\"delta\":{t}}}\n\nevent: response.completed\ndata: {{\"type\":\"response.completed\",\"response\":{{\"output\":[{{\"type\":\"message\",\"id\":\"msg_1\",\"status\":\"completed\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":{t}}}]}}],\"usage\":{{\"input_tokens\":{input},\"input_tokens_details\":{{\"cached_tokens\":{cached}}},\"output_tokens\":3}}}}}}\n\n",
        t = serde_json::to_string(text).unwrap()
    )
}

#[tokio::test]
async fn openai_effort_change_uses_configuration_update() {
    let m = mock(vec![Reply::Sse(oa_text("one", 1500, 0)), Reply::Sse(oa_text("two", 1600, 1500))]).await;
    let st = state(ProviderId::Openai, m.port, None);
    let (emit, ev) = collector();
    run(st.clone(), params(ProviderId::Openai, "gpt-6-astra", "first", Some(Effort::Medium), Mode::Office), Cancel::default(), emit.clone()).await;
    run(st.clone(), params(ProviderId::Openai, "gpt-6-astra", "second", Some(Effort::Xhigh), Mode::Office), Cancel::default(), emit).await;
    let bodies = m.bodies.lock().unwrap();
    assert_eq!(bodies[0]["reasoning"]["effort"], "medium");
    assert_eq!(bodies[1]["reasoning"]["effort"], "medium", "request-level effort stays for cache reuse");
    let input = bodies[1]["input"].as_array().unwrap();
    assert_eq!(input[2]["type"], "configuration_update");
    assert_eq!(input[2]["reasoning"]["effort"], "xhigh");
    let a = bodies[0]["input"].as_array().unwrap();
    assert!(a.iter().zip(input.iter()).all(|(x, y)| x == y), "append-only input");
    assert_eq!(bodies[0]["instructions"], bodies[1]["instructions"]);
    assert_eq!(bodies[0]["prompt_cache_key"], bodies[1]["prompt_cache_key"]);
    // Second turn reports a 1500/1600 cache hit.
    let usages: Vec<Usage> = ev.lock().unwrap().iter().filter_map(|e| if let AgentEvent::Usage { turn, .. } = e { Some(turn.clone()) } else { None }).collect();
    assert!((usages[1].hit_rate().unwrap() - 1500.0 / 1600.0).abs() < 1e-9);
    // Settings remember the per-model effort.
    assert_eq!(st.settings().efforts.get("openai:gpt-6-astra"), Some(&Effort::Xhigh));
}

#[test]
fn plan_effort_param_models_change_request_level() {
    let caps = crate::providers::testutil::caps(ProviderId::Deepseek, "deepseek-flash");
    let mut a = Message::new(Role::Assistant);
    a.provider = Some(ProviderId::Deepseek);
    a.model = Some("deepseek-flash".into());
    a.effort = Some(Effort::High);
    let (req, ctl) = plan_effort(&[a.clone()], ProviderId::Deepseek, &caps, Some(Effort::Max));
    assert_eq!(req, Some(Effort::Max));
    assert!(matches!(ctl, Some(Control::Effort { in_band: false, .. })));
    let (req, ctl) = plan_effort(&[a], ProviderId::Deepseek, &caps, Some(Effort::High));
    assert_eq!(req, Some(Effort::High));
    assert!(ctl.is_none());
    // Unsupported value never reaches the request.
    let (req, _) = plan_effort(&[], ProviderId::Deepseek, &caps, Some(Effort::Medium));
    assert_eq!(req, Some(Effort::High));
}

#[test]
fn trim_context_only_touches_old_tool_output() {
    let mut h = Vec::new();
    for i in 0..30 {
        let mut t = Message::new(Role::Tool);
        t.tool_results.push(ToolResult { call_id: format!("c{i}"), name: "read_file".into(), content: "x".repeat(3000), is_error: false, status: "success".into(), diff: None, images: vec![] });
        h.push(t);
    }
    let n = trim_context(&mut h, 20_000, &[]);
    assert!(n > 0);
    assert!(h[0].tool_results[0].content.starts_with("[older tool output removed"));
    assert_eq!(h[29].tool_results[0].content.len(), 3000, "recent output kept");
    assert_eq!(trim_context(&mut h, u64::MAX, &[]), 0);
}

#[test]
fn titles() {
    assert_eq!(title_from("\n\n# Fix the login bug in auth.rs\nmore"), "Fix the login bug in auth.rs");
    assert_eq!(title_from(&"长".repeat(60)).chars().count(), 41);
}

#[tokio::test]
async fn generate_image_tool_saves_and_reports_image() {
    use base64::Engine;
    let png = base64::engine::general_purpose::STANDARD.encode([0x89u8, b'P', b'N', b'G', 1, 2, 3]);
    let image_json = format!(r#"{{"created":1,"data":[{{"b64_json":"{png}"}}],"output_format":"png","usage":{{"input_tokens":20,"output_tokens":4160,"total_tokens":4180}}}}"#);
    let m = mock(vec![
        Reply::Sse(ds_tool("generate_image", "{\"prompt\":\"a red fox, watercolor\",\"size\":\"1024x1024\"}")),
        Reply::Status(200, image_json),
        Reply::Sse(ds_text("Here is your fox.", 0, 10)),
    ])
    .await;
    let st = state(ProviderId::Deepseek, m.port, None);
    lock(&st.key_override).insert(ProviderId::Openai, "test-openai".into());
    st.update_settings(|s| {
        s.base_urls.insert("openai".into(), format!("http://127.0.0.1:{}", m.port));
    })
    .unwrap();
    let (emit, _ev) = collector();
    run(st.clone(), params(ProviderId::Deepseek, "deepseek-flash", "draw a fox", None, Mode::Office), Cancel::default(), emit).await;
    let bodies = m.bodies.lock().unwrap();
    let names: Vec<_> = bodies[0]["tools"].as_array().unwrap().iter().map(|t| t["function"]["name"].as_str().unwrap().to_string()).collect();
    assert!(names.contains(&"generate_image".to_string()) && names.contains(&"edit_image".to_string()));
    assert_eq!(bodies[1]["model"], "gpt-image-2.6-flare");
    assert_eq!(bodies[1]["prompt"], "a red fox, watercolor");
    assert_eq!(bodies[1]["size"], "1024x1024");
    let conv = st.db.get_conversation("conv-1").unwrap();
    let r = &conv.messages[2].tool_results[0];
    assert_eq!(r.status, "success", "{}", r.content);
    assert_eq!(r.images.len(), 1);
    let file = st.data_dir.join("images").join(&r.images[0]);
    assert_eq!(std::fs::read(&file).unwrap(), vec![0x89u8, b'P', b'N', b'G', 1, 2, 3]);
    // Only the id (not the image data) goes back to the model.
    let follow_up = bodies[2]["messages"].as_array().unwrap();
    let tool_msg = follow_up.iter().find(|m| m["role"] == "tool").unwrap();
    assert!(tool_msg["content"].as_str().unwrap().contains(&r.images[0]));
    assert!(tool_msg["content"].as_str().unwrap().len() < 600);
    assert!(crate::images::data_url(&st.data_dir.join("images"), &r.images[0]).unwrap().starts_with("data:image/png;base64,"));
}

#[tokio::test]
async fn image_tools_hidden_without_openai_key_or_when_disabled() {
    let m = mock(vec![Reply::Sse(ds_text("ok", 0, 10)), Reply::Sse(ds_text("ok", 0, 10))]).await;
    let st = state(ProviderId::Deepseek, m.port, None);
    let (emit, _ev) = collector();
    run(st.clone(), params(ProviderId::Deepseek, "deepseek-flash", "hi", None, Mode::Office), Cancel::default(), emit.clone()).await;
    lock(&st.key_override).insert(ProviderId::Openai, "k".into());
    st.update_settings(|s| s.image_generation = false).unwrap();
    run(st.clone(), params(ProviderId::Deepseek, "deepseek-flash", "hi again", None, Mode::Office), Cancel::default(), emit).await;
    for b in m.bodies.lock().unwrap().iter() {
        let names: Vec<_> = b["tools"].as_array().unwrap().iter().map(|t| t["function"]["name"].as_str().unwrap().to_string()).collect();
        assert!(!names.contains(&"generate_image".to_string()));
    }
}


#[tokio::test]
async fn auto_approve_edits_mode_writes_without_prompt_but_commands_still_ask() {
    let ws = temp_ws();
    let m = mock(vec![
        Reply::Sse(ds_tool("write_file", "{\"path\":\"auto.txt\",\"content\":\"ok\"}")),
        Reply::Sse(ds_text("Written.", 0, 10)),
    ])
    .await;
    let st = state(ProviderId::Deepseek, m.port, Some(&ws));
    st.update_settings(|s| s.auto_approve_edits = true).unwrap();
    let (emit, ev) = collector();
    run(st.clone(), params(ProviderId::Deepseek, "deepseek-v4-pro", "write", None, Mode::Coding), Cancel::default(), emit).await;
    assert_eq!(std::fs::read_to_string(ws.join("auto.txt")).unwrap(), "ok");
    let asked = ev.lock().unwrap().iter().any(|e| matches!(e, AgentEvent::ToolStatus { approval: Some(_), .. }));
    assert!(!asked, "no approval prompt for workspace edits in this mode");
    // Commands are never auto-approved by this mode.
    let req = ApprovalRequest { kind: ApprovalKind::RunCommand, target: "dir".into(), diff: None, warning: None };
    assert!(!session_allows(&st, "conv-1", Some(&ws), &req));
}

/// Regression: a tool call whose result was never saved (Ycode closed or was
/// updated while the tool was pending/running) made every later request fail
/// with "No tool output found for function call ...". The history sent to the
/// provider must contain a result for every call.
#[tokio::test]
async fn interrupted_tool_call_gets_placeholder_result() {
    let m = mock(vec![Reply::Sse(ds_text("Continuing.", 0, 10))]).await;
    let st = state(ProviderId::Deepseek, m.port, None);
    st.db.create_conversation("conv-1", Mode::Coding, None).unwrap();
    let mut u = Message::new(Role::User);
    u.text = "run it".into();
    st.db.upsert_message("conv-1", &u).unwrap();
    let mut a = Message::new(Role::Assistant);
    a.provider = Some(ProviderId::Deepseek);
    a.model = Some("deepseek-flash".into());
    a.raw = Some(RawReplay { provider: ProviderId::Deepseek, model: "deepseek-flash".into(), data: serde_json::json!({"reasoning_content": ""}) });
    a.tool_calls = vec![
        ToolCall { id: "call_a".into(), name: "list_dir".into(), arguments: serde_json::json!({}) },
        ToolCall { id: "call_b".into(), name: "read_file".into(), arguments: serde_json::json!({"path": "x"}) },
    ];
    st.db.upsert_message("conv-1", &a).unwrap();
    // Only the first call got a result before the app went away.
    let mut t = Message::new(Role::Tool);
    t.tool_results.push(ToolResult { call_id: "call_a".into(), name: "list_dir".into(), content: "x".into(), is_error: false, status: "success".into(), diff: None, images: vec![] });
    st.db.upsert_message("conv-1", &t).unwrap();
    let (emit, ev) = collector();
    run(st.clone(), params(ProviderId::Deepseek, "deepseek-flash", "continue", None, Mode::Coding), Cancel::default(), emit).await;
    assert!(!ev.lock().unwrap().iter().any(|e| matches!(e, AgentEvent::Error { .. })), "request must succeed");
    let body = &m.bodies.lock().unwrap()[0];
    let msgs = body["messages"].as_array().unwrap();
    let tool_ids: Vec<&str> = msgs.iter().filter(|m| m["role"] == "tool").map(|m| m["tool_call_id"].as_str().unwrap()).collect();
    assert_eq!(tool_ids, vec!["call_a", "call_b"], "every call has a result, in order, before the next user turn");
    let b = msgs.iter().find(|m| m["tool_call_id"] == "call_b").unwrap();
    assert!(b["content"].as_str().unwrap().contains("interrupted"));
    // Saved history is not rewritten.
    assert_eq!(st.db.get_conversation("conv-1").unwrap().messages[2].tool_results.len(), 1);
}

#[test]
fn request_history_repairs_calls_without_any_result() {
    let mut u1 = Message::new(Role::User);
    u1.text = "a".into();
    let mut a = Message::new(Role::Assistant);
    a.tool_calls = vec![ToolCall { id: "c1".into(), name: "run_command".into(), arguments: serde_json::json!({}) }];
    let mut u2 = Message::new(Role::User);
    u2.text = "b".into();
    let h = request_history(&[u1, a.clone(), u2]);
    assert_eq!(h.iter().map(|m| m.role).collect::<Vec<_>>(), vec![Role::User, Role::Assistant, Role::Tool, Role::User]);
    assert_eq!(h[2].tool_results[0].call_id, "c1");
    assert_eq!(h[2].tool_results[0].status, "cancelled");
    // Deterministic, so the cached prefix stays stable across requests.
    assert_eq!(request_history(&[a.clone()]), request_history(&[a]));
}
