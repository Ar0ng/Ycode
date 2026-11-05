//! Anthropic Messages API adapter (`POST /v1/messages`, raw HTTP).

use super::registry::{self, ModelCaps};
use super::sse::SseEvent;
use super::{BuiltRequest, Decoder, StopKind, StreamEvent, TurnOutput, TurnRequest, parse_args};
use crate::error::{ProviderError, ProviderErrorKind};
use crate::model::{Control, Effort, ProviderId, Role, ToolCall, Usage};
use serde_json::{Value, json};

pub const API_VERSION: &str = "2026-10-21";
const BETA_EFFORT_MSG: &str = "mid-conversation-output-config-2026-07-01";
const BETA_FALLBACK: &str = "server-side-fallback-2026-07-01";

pub fn build(req: &TurnRequest) -> BuiltRequest {
    let mut messages: Vec<Value> = Vec::new();
    let mut uses_effort_msg = false;
    for m in req.history {
        match m.role {
            Role::User => messages.push(json!({ "role": "user", "content": [{ "type": "text", "text": m.user_content_text() }] })),
            Role::Assistant => {
                let content = if let Some(Value::Array(blocks)) = m.raw.as_ref().filter(|r| r.provider == ProviderId::Anthropic).map(|r| &r.data) {
                    // Thinking blocks are passed back unchanged (any Claude model; the API
                    // drops blocks the current model can't read, unbilled).
                    blocks.clone()
                } else {
                    let mut c = Vec::new();
                    if !m.text.is_empty() {
                        c.push(json!({ "type": "text", "text": m.text }));
                    }
                    for tc in &m.tool_calls {
                        c.push(json!({ "type": "tool_use", "id": tc.id, "name": tc.name, "input": tool_input(&tc.arguments) }));
                    }
                    c
                };
                if content.is_empty() {
                    // An empty assistant turn (e.g. stopped before any output) is invalid.
                    messages.push(json!({ "role": "assistant", "content": [{ "type": "text", "text": "(no output)" }] }));
                } else {
                    messages.push(json!({ "role": "assistant", "content": content }));
                }
            }
            Role::Tool => {
                let blocks: Vec<Value> = m
                    .tool_results
                    .iter()
                    .map(|r| json!({ "type": "tool_result", "tool_use_id": r.call_id, "content": r.content, "is_error": r.is_error }))
                    .collect();
                messages.push(json!({ "role": "user", "content": blocks }));
            }
            Role::Control => {
                if let Some(Control::Effort { provider: ProviderId::Anthropic, model, effort, in_band: true }) = &m.control {
                    if model == req.model {
                        uses_effort_msg = true;
                        messages.push(json!({ "role": "system", "content": [], "output_config": { "effort": effort.as_str() } }));
                    }
                }
            }
        }
    }
    let mut system: Vec<Value> = req.system.iter().map(|s| json!({ "type": "text", "text": s })).collect();
    if let Some(last) = system.last_mut() {
        // Explicit breakpoint covering tools + system (stable prefix).
        last["cache_control"] = json!({ "type": "ephemeral" });
    }
    let mut body = json!({
        "model": req.model,
        "max_tokens": req.max_output,
        "stream": true,
        "system": system,
        "messages": messages,
        // Automatic breakpoint that follows the growing conversation.
        "cache_control": { "type": "ephemeral" },
    });
    if !req.tools.is_empty() {
        body["tools"] = Value::Array(
            req.tools
                .iter()
                .map(|t| json!({ "name": t.name, "description": t.description, "input_schema": t.parameters, "eager_input_streaming": true }))
                .collect(),
        );
        body["tool_choice"] = json!({ "type": "auto" });
    }
    if req.caps.adaptive_thinking {
        body["thinking"] = json!({ "type": "adaptive", "display": "summarized" });
    }
    if let Some(e) = req.effort.filter(|e| *e != Effort::None) {
        body["output_config"] = json!({ "effort": e.as_str() });
    }
    let mut betas: Vec<&str> = Vec::new();
    if uses_effort_msg {
        betas.push(BETA_EFFORT_MSG);
    }
    if req.fallbacks && req.caps.server_fallbacks {
        betas.push(BETA_FALLBACK);
        body["fallbacks"] = json!("default");
    }
    let headers = if betas.is_empty() { vec![] } else { vec![("anthropic-beta".to_string(), betas.join(","))] };
    BuiltRequest { path: "/v1/messages".into(), headers, body }
}

fn tool_input(v: &Value) -> Value {
    if v.is_object() { v.clone() } else { json!({}) }
}

/// After a mid-output fallback, blocks before the last `fallback` marker that the
/// fallback model must not see are dropped (thinking, redacted_thinking, tool_use,
/// unpaired server_tool_use, unknown internal blocks). Text survives.
pub fn sanitize_after_fallback(blocks: Vec<Value>) -> Vec<Value> {
    let Some(last_fb) = blocks.iter().rposition(|b| b.get("type").and_then(|t| t.as_str()) == Some("fallback")) else {
        return blocks;
    };
    blocks
        .into_iter()
        .enumerate()
        .filter(|(i, b)| {
            if *i >= last_fb {
                return true;
            }
            matches!(b.get("type").and_then(|t| t.as_str()), Some("text") | Some("fallback"))
        })
        .map(|(_, b)| b)
        .collect()
}

pub struct AnthropicDecoder {
    blocks: Vec<Value>,
    /// Accumulated partial JSON per tool_use block index.
    partial_json: Vec<String>,
    text: String,
    reasoning: String,
    usage: Usage,
    stop: Option<String>,
    done: bool,
}

impl AnthropicDecoder {
    pub fn new(_model: &str) -> Self {
        Self { blocks: vec![], partial_json: vec![], text: String::new(), reasoning: String::new(), usage: Usage::default(), stop: None, done: false }
    }

    fn compute_usage_totals(&mut self) {
        let u = &mut self.usage;
        // input_tokens from the API excludes cache read/creation tokens.
        let total = u.input_tokens + u.cached_input_tokens + u.cache_write_tokens.unwrap_or(0);
        u.eligible_input_tokens = Some(total);
        u.input_tokens = total;
    }

    fn output(&self, finished: bool) -> TurnOutput {
        let blocks = sanitize_after_fallback(self.blocks.clone());
        let mut text = String::new();
        let mut calls = Vec::new();
        for b in &blocks {
            match b.get("type").and_then(|t| t.as_str()) {
                Some("text") => text.push_str(b.get("text").and_then(|t| t.as_str()).unwrap_or("")),
                Some("tool_use") if finished => calls.push(ToolCall {
                    id: b.get("id").and_then(|s| s.as_str()).unwrap_or_default().into(),
                    name: b.get("name").and_then(|s| s.as_str()).unwrap_or_default().into(),
                    arguments: b.get("input").cloned().unwrap_or(json!({})),
                }),
                _ => {}
            }
        }
        let stop = match self.stop.as_deref() {
            Some("tool_use") => StopKind::ToolUse,
            Some("end_turn") | Some("stop_sequence") => StopKind::End,
            Some("max_tokens") => StopKind::MaxTokens,
            Some("refusal") => StopKind::Refusal,
            Some(o) => StopKind::Other(o.into()),
            None => StopKind::Other("stopped".into()),
        };
        TurnOutput {
            text,
            reasoning: if self.reasoning.is_empty() { None } else { Some(self.reasoning.clone()) },
            tool_calls: calls,
            raw: Value::Array(blocks),
            usage: self.usage.clone(),
            stop,
        }
    }
}

fn read_usage(u: &Value, into: &mut Usage) {
    let g = |k: &str| u.get(k).and_then(|v| v.as_u64());
    if let Some(v) = g("input_tokens") {
        into.input_tokens = v;
    }
    if let Some(v) = g("output_tokens") {
        into.output_tokens = v;
    }
    if let Some(v) = g("cache_read_input_tokens") {
        into.cached_input_tokens = v;
    }
    if let Some(v) = g("cache_creation_input_tokens") {
        into.cache_write_tokens = Some(v);
    }
}

impl Decoder for AnthropicDecoder {
    fn on_event(&mut self, ev: SseEvent, out: &mut Vec<StreamEvent>) -> Result<(), ProviderError> {
        let Ok(v) = serde_json::from_str::<Value>(&ev.data) else { return Ok(()) };
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match ty {
            "message_start" => {
                if let Some(u) = v.pointer("/message/usage") {
                    read_usage(u, &mut self.usage);
                }
            }
            "content_block_start" => {
                let idx = v.get("index").and_then(|i| i.as_u64()).unwrap_or(self.blocks.len() as u64) as usize;
                let block = v.get("content_block").cloned().unwrap_or(json!({}));
                while self.blocks.len() <= idx {
                    self.blocks.push(json!({}));
                    self.partial_json.push(String::new());
                }
                match block.get("type").and_then(|t| t.as_str()) {
                    Some("tool_use") => out.push(StreamEvent::ToolStart {
                        id: block.get("id").and_then(|s| s.as_str()).unwrap_or_default().into(),
                        name: block.get("name").and_then(|s| s.as_str()).unwrap_or_default().into(),
                    }),
                    Some("fallback") => out.push(StreamEvent::Notice { message: "fallback".into() }),
                    _ => {}
                }
                self.blocks[idx] = block;
            }
            "content_block_delta" => {
                let idx = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                let Some(block) = self.blocks.get_mut(idx) else { return Ok(()) };
                let d = &v["delta"];
                match d.get("type").and_then(|t| t.as_str()) {
                    Some("text_delta") => {
                        let t = d.get("text").and_then(|s| s.as_str()).unwrap_or("");
                        let cur = block.get("text").and_then(|s| s.as_str()).unwrap_or("").to_string();
                        block["text"] = json!(cur + t);
                        self.text.push_str(t);
                        out.push(StreamEvent::Text { delta: t.into() });
                    }
                    Some("thinking_delta") => {
                        let t = d.get("thinking").and_then(|s| s.as_str()).unwrap_or("");
                        let cur = block.get("thinking").and_then(|s| s.as_str()).unwrap_or("").to_string();
                        block["thinking"] = json!(cur + t);
                        self.reasoning.push_str(t);
                        out.push(StreamEvent::Reasoning { delta: t.into() });
                    }
                    Some("signature_delta") => {
                        let s = d.get("signature").and_then(|s| s.as_str()).unwrap_or("");
                        let cur = block.get("signature").and_then(|s| s.as_str()).unwrap_or("").to_string();
                        block["signature"] = json!(cur + s);
                    }
                    Some("input_json_delta") => {
                        self.partial_json[idx].push_str(d.get("partial_json").and_then(|s| s.as_str()).unwrap_or(""));
                    }
                    _ => {}
                }
            }
            "content_block_stop" => {
                let idx = v.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                if let Some(block) = self.blocks.get_mut(idx) {
                    if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                        let raw = std::mem::take(&mut self.partial_json[idx]);
                        // Eager input streaming: the API no longer validates input → parse
                        // defensively; invalid JSON becomes {"_raw": ...} and is rejected
                        // by the tool layer with a precise error.
                        block["input"] = if raw.is_empty() { block.get("input").cloned().unwrap_or(json!({})) } else { parse_args(&raw) };
                    }
                }
            }
            "message_delta" => {
                if let Some(s) = v.pointer("/delta/stop_reason").and_then(|s| s.as_str()) {
                    self.stop = Some(s.into());
                }
                if let Some(u) = v.get("usage") {
                    read_usage(u, &mut self.usage);
                }
            }
            "message_stop" => {
                self.compute_usage_totals();
                self.done = true;
            }
            "error" => {
                let et = v.pointer("/error/type").and_then(|s| s.as_str()).unwrap_or("");
                let msg = v.pointer("/error/message").and_then(|s| s.as_str()).unwrap_or("stream error");
                let kind = match et {
                    "overloaded_error" => ProviderErrorKind::Overloaded,
                    "rate_limit_error" => ProviderErrorKind::RateLimited,
                    "api_error" => ProviderErrorKind::Server,
                    "authentication_error" => ProviderErrorKind::InvalidKey,
                    "permission_error" => ProviderErrorKind::ModelNoAccess,
                    "not_found_error" => ProviderErrorKind::ModelNotFound,
                    "billing_error" => ProviderErrorKind::InsufficientBalance,
                    _ => ProviderErrorKind::Unknown,
                };
                return Err(ProviderError::new(kind, msg));
            }
            _ => {}
        }
        Ok(())
    }

    fn done(&self) -> bool {
        self.done
    }

    fn partial(&self) -> TurnOutput {
        let mut o = self.output(false);
        // Partial thinking / tool_use blocks are not valid replay data.
        o.raw = Value::Null;
        o
    }

    fn finish(self: Box<Self>) -> Result<TurnOutput, ProviderError> {
        let mut o = self.output(true);
        if o.stop == StopKind::Refusal {
            // A refused turn's partial content must not be treated as complete; drop
            // replay data so the next request starts clean.
            o.raw = Value::Null;
        }
        Ok(o)
    }
}

pub fn parse_models(v: &Value) -> Vec<ModelCaps> {
    let mut out = Vec::new();
    for m in v.get("data").and_then(|d| d.as_array()).into_iter().flatten() {
        let Some(id) = m.get("id").and_then(|s| s.as_str()) else { continue };
        if m.get("lifecycle").and_then(|l| l.as_str()) == Some("retired") {
            continue;
        }
        let mut c = registry::unknown(ProviderId::Anthropic, id);
        c.source = "remote".into();
        c.note = None;
        if let Some(n) = m.get("display_name").and_then(|s| s.as_str()) {
            c.display_name = n.into();
        }
        c.context_window = m.get("max_input_tokens").and_then(|x| x.as_u64()).filter(|x| *x > 0).unwrap_or(200_000);
        c.max_output = m.get("max_tokens").and_then(|x| x.as_u64()).filter(|x| *x > 0).unwrap_or(16_000);
        let caps = &m["capabilities"];
        let sup = |p: &str| caps.pointer(p).and_then(|x| x.as_bool()).unwrap_or(false);
        c.vision = sup("/image_input/supported");
        c.adaptive_thinking = sup("/thinking/types/adaptive/supported");
        if sup("/effort/supported") {
            let mut e = Vec::new();
            for (k, lvl) in [("low", Effort::Low), ("medium", Effort::Medium), ("high", Effort::High), ("xhigh", Effort::Xhigh), ("max", Effort::Max)] {
                if sup(&format!("/effort/{k}/supported")) {
                    e.push(lvl);
                }
            }
            c.efforts = e;
            // Docs: omitting effort = model default; we don't know it for unlisted
            // models, so pick `high` only if offered (documented default for most).
            c.default_effort = if c.efforts.contains(&Effort::High) { Some(Effort::High) } else { c.efforts.first().copied() };
        }
        if m.get("lifecycle").and_then(|l| l.as_str()) == Some("deprecated") {
            c.note = Some("deprecated".into());
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Message, RawReplay};
    use crate::providers::sse::SseParser;
    use crate::providers::testutil::*;

    #[test]
    fn builds_messages_body_with_cache_and_effort() {
        let c = caps(ProviderId::Anthropic, "claude-opus-5-5");
        let sys = vec!["base".to_string(), "mode".to_string()];
        let tools = vec![crate::providers::ToolSpec { name: "list_dir", description: "d", parameters: json!({"type":"object"}) }];
        let h = sample_history(ProviderId::Anthropic, "claude-opus-5-5");
        let b = build(&req(ProviderId::Anthropic, &c, &sys, &tools, &h));
        assert_eq!(b.path, "/v1/messages");
        assert_eq!(b.body["system"][1]["cache_control"]["type"], "ephemeral");
        assert!(b.body["system"][0].get("cache_control").is_none());
        assert_eq!(b.body["cache_control"]["type"], "ephemeral");
        assert_eq!(b.body["thinking"]["type"], "adaptive");
        assert_eq!(b.body["output_config"]["effort"], "medium");
        assert_eq!(b.body["tool_choice"]["type"], "auto");
        assert_eq!(b.body["tools"][0]["eager_input_streaming"], true);
        assert_eq!(b.body["messages"][1]["content"][1]["type"], "tool_use");
        assert_eq!(b.body["messages"][2]["content"][0]["type"], "tool_result");
        assert_eq!(b.body["fallbacks"], "default");
        assert_eq!(b.headers[0].1, "server-side-fallback-2026-07-01");
        assert!(b.body.get("temperature").is_none());
    }

    #[test]
    fn per_message_effort_keeps_message_prefix() {
        let c = caps(ProviderId::Anthropic, "claude-sonnet-5-5");
        let h1 = sample_history(ProviderId::Anthropic, "claude-sonnet-5-5");
        let mut r1 = req(ProviderId::Anthropic, &c, &[], &[], &h1);
        r1.fallbacks = false;
        let b1 = build(&r1);
        let mut h2 = h1.clone();
        let mut ctl = Message::new(Role::Control);
        ctl.control = Some(Control::Effort { provider: ProviderId::Anthropic, model: "claude-sonnet-5-5".into(), effort: Effort::Max, in_band: true });
        h2.push(ctl);
        let mut r2 = req(ProviderId::Anthropic, &c, &[], &[], &h2);
        r2.fallbacks = false;
        let b2 = build(&r2);
        assert!(messages_prefix_stable(&b1.body, &b2.body, "messages"));
        assert_eq!(b2.body["messages"][3]["role"], "system");
        assert_eq!(b2.body["messages"][3]["output_config"]["effort"], "max");
        assert_eq!(b1.body["output_config"], b2.body["output_config"]);
        assert_eq!(b2.headers[0].1, "mid-conversation-output-config-2026-07-01");
    }

    #[test]
    fn replays_raw_blocks_unchanged() {
        let c = caps(ProviderId::Anthropic, "claude-opus-5-5");
        let mut h = sample_history(ProviderId::Anthropic, "claude-opus-5-5");
        let blocks = json!([{"type":"thinking","thinking":"","signature":"SIG"},{"type":"tool_use","id":"call_1","name":"list_dir","input":{"path":"."}}]);
        h[1].raw = Some(RawReplay { provider: ProviderId::Anthropic, model: "claude-opus-5-5".into(), data: blocks.clone() });
        let b = build(&req(ProviderId::Anthropic, &c, &[], &[], &h));
        assert_eq!(b.body["messages"][1]["content"], blocks);
    }

    #[test]
    fn fallback_sanitizer_drops_pre_boundary_internal_blocks() {
        let blocks = vec![
            json!({"type":"thinking","thinking":"x"}),
            json!({"type":"text","text":"partial"}),
            json!({"type":"tool_use","id":"t"}),
            json!({"type":"fallback","from":{"model":"a"},"to":{"model":"b"}}),
            json!({"type":"thinking","thinking":"y"}),
            json!({"type":"text","text":"rest"}),
        ];
        let out = sanitize_after_fallback(blocks);
        let types: Vec<_> = out.iter().map(|b| b["type"].as_str().unwrap().to_string()).collect();
        assert_eq!(types, vec!["text", "fallback", "thinking", "text"]);
    }

    #[test]
    fn decodes_stream() {
        let s = concat!(
            "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"m\",\"usage\":{\"input_tokens\":100,\"cache_read_input_tokens\":3000,\"cache_creation_input_tokens\":200,\"output_tokens\":1}}}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"plan\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"signature_delta\",\"signature\":\"S\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hi\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n",
            "event: ping\ndata: {\"type\":\"ping\"}\n\n",
            "event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"tool_use\",\"id\":\"tu1\",\"name\":\"read_file\",\"input\":{}}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\":\"}}\n\n",
            "event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"a\\\"}\"}}\n\n",
            "event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":2}\n\n",
            "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":40}}\n\n",
            "event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
        );
        let mut d = Box::new(AnthropicDecoder::new("claude-opus-5-5"));
        let mut p = SseParser::new();
        let mut ev = vec![];
        for e in p.feed(s.as_bytes()) {
            d.on_event(e, &mut ev).unwrap();
        }
        assert!(d.done());
        let o = d.finish().unwrap();
        assert_eq!(o.text, "Hi");
        assert_eq!(o.reasoning.as_deref(), Some("plan"));
        assert_eq!(o.tool_calls[0].arguments["path"], "a");
        assert_eq!(o.stop, StopKind::ToolUse);
        assert_eq!(o.raw[0]["signature"], "S");
        assert_eq!(o.raw[2]["input"]["path"], "a");
        assert_eq!(o.usage.input_tokens, 3300);
        assert_eq!(o.usage.cached_input_tokens, 3000);
        assert_eq!(o.usage.cache_write_tokens, Some(200));
        assert_eq!(o.usage.output_tokens, 40);
        assert!((o.usage.hit_rate().unwrap() - 3000.0 / 3300.0).abs() < 1e-9);
    }

    #[test]
    fn parses_models_capabilities() {
        let v = json!({"data":[{"id":"claude-x","display_name":"Claude X","lifecycle":"active","max_input_tokens":1000000,"max_tokens":128000,
            "capabilities":{"image_input":{"supported":true},"thinking":{"supported":true,"types":{"adaptive":{"supported":true}}},
            "effort":{"supported":true,"low":{"supported":true},"medium":{"supported":true},"high":{"supported":true},"max":{"supported":true},"xhigh":null}}},
            {"id":"old","lifecycle":"retired"}],"has_more":false});
        let m = parse_models(&v);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].efforts, vec![Effort::Low, Effort::Medium, Effort::High, Effort::Max]);
        assert!(m[0].adaptive_thinking && m[0].vision);
    }
}
