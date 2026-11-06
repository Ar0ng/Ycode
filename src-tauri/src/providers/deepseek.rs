//! DeepSeek adapter — Chat Completions (`POST /chat/completions`) with DeepSeek's
//! `thinking` / `reasoning_effort` fields and `reasoning_content` replay.

use super::registry::{self, ModelCaps};
use super::sse::SseEvent;
use super::{BuiltRequest, Decoder, StopKind, StreamEvent, TurnOutput, TurnRequest, parse_args};
use crate::error::{ProviderError, ProviderErrorKind};
use crate::model::{Effort, ProviderId, Role, ToolCall, Usage};
use serde_json::{Value, json};

const MAX_TOKENS_CAP: u64 = 393_216;

pub fn build(req: &TurnRequest) -> BuiltRequest {
    let mut messages: Vec<Value> = vec![json!({ "role": "system", "content": req.system.join("\n\n") })];
    for m in req.history {
        match m.role {
            Role::User => messages.push(json!({ "role": "user", "content": m.user_content_text() })),
            Role::Assistant => {
                let mut a = json!({
                    "role": "assistant",
                    "content": if m.text.is_empty() { Value::Null } else { Value::String(m.text.clone()) },
                });
                // reasoning_content must be replayed for every earlier turn when tools are
                // present; it is model-family data, so any DeepSeek turn qualifies.
                if let Some(r) = m.raw.as_ref().filter(|r| r.provider == ProviderId::Deepseek)
                    && let Some(rc) = r.data.get("reasoning_content").and_then(|s| s.as_str()) {
                        a["reasoning_content"] = json!(rc);
                    }
                if !m.tool_calls.is_empty() {
                    a["tool_calls"] = Value::Array(
                        m.tool_calls
                            .iter()
                            .map(|c| json!({ "id": c.id, "type": "function", "function": { "name": c.name, "arguments": c.arguments.to_string() } }))
                            .collect(),
                    );
                }
                messages.push(a);
            }
            Role::Tool => {
                for r in &m.tool_results {
                    messages.push(json!({ "role": "tool", "tool_call_id": r.call_id, "content": r.content }));
                }
            }
            Role::Control => {}
        }
    }
    let mut body = json!({
        "model": req.model,
        "messages": messages,
        "stream": true,
        "stream_options": { "include_usage": true },
        "max_tokens": req.max_output.min(MAX_TOKENS_CAP),
    });
    if !req.tools.is_empty() {
        body["tools"] = Value::Array(
            req.tools
                .iter()
                .map(|t| json!({ "type": "function", "function": { "name": t.name, "description": t.description, "parameters": t.parameters } }))
                .collect(),
        );
    }
    match req.effort {
        Some(Effort::None) => body["thinking"] = json!({ "type": "disabled" }),
        Some(e) => {
            body["thinking"] = json!({ "type": "enabled" });
            body["reasoning_effort"] = json!(e.as_str());
        }
        None => {}
    }
    BuiltRequest { path: "/chat/completions".into(), headers: vec![], body }
}

#[derive(Default)]
struct PartialCall {
    id: String,
    name: String,
    args: String,
}

pub struct DeepSeekDecoder {
    text: String,
    reasoning: String,
    calls: Vec<PartialCall>,
    finish: Option<String>,
    usage: Usage,
    done: bool,
}

impl DeepSeekDecoder {
    pub fn new(_model: &str) -> Self {
        Self { text: String::new(), reasoning: String::new(), calls: vec![], finish: None, usage: Usage::default(), done: false }
    }
    fn output(&self, include_calls: bool) -> TurnOutput {
        let tool_calls: Vec<ToolCall> = if include_calls {
            self.calls.iter().map(|c| ToolCall { id: c.id.clone(), name: c.name.clone(), arguments: parse_args(&c.args) }).collect()
        } else {
            vec![]
        };
        let stop = match self.finish.as_deref() {
            Some("tool_calls") => StopKind::ToolUse,
            _ if !tool_calls.is_empty() => StopKind::ToolUse,
            Some("stop") => StopKind::End,
            Some("length") => StopKind::MaxTokens,
            Some("content_filter") => StopKind::Refusal,
            Some(o) => StopKind::Other(o.into()),
            None => StopKind::Other("stopped".into()),
        };
        TurnOutput {
            text: self.text.clone(),
            reasoning: if self.reasoning.is_empty() { None } else { Some(self.reasoning.clone()) },
            tool_calls,
            raw: json!({ "reasoning_content": self.reasoning }),
            usage: self.usage.clone(),
            stop,
        }
    }
}

pub fn parse_usage(u: &Value) -> Usage {
    let g = |k: &str| u.get(k).and_then(|v| v.as_u64());
    let hit = g("prompt_cache_hit_tokens");
    let miss = g("prompt_cache_miss_tokens");
    Usage {
        input_tokens: g("prompt_tokens").unwrap_or(0),
        output_tokens: g("completion_tokens").unwrap_or(0),
        cached_input_tokens: hit.unwrap_or(0),
        cache_write_tokens: None,
        reasoning_tokens: u.pointer("/completion_tokens_details/reasoning_tokens").and_then(|v| v.as_u64()),
        // Hit rate is only reported when DeepSeek gives both halves of the split.
        eligible_input_tokens: hit.zip(miss).and_then(|(h, m)| h.checked_add(m)),
    }
}

impl Decoder for DeepSeekDecoder {
    fn on_event(&mut self, ev: SseEvent, out: &mut Vec<StreamEvent>) -> Result<(), ProviderError> {
        let data = ev.data.trim();
        if data == "[DONE]" {
            self.done = true;
            return Ok(());
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else { return Ok(()) };
        if let Some(err) = v.get("error") {
            let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("stream error");
            return Err(ProviderError::new(ProviderErrorKind::Server, msg));
        }
        if let Some(u) = v.get("usage").filter(|u| !u.is_null()) {
            self.usage = parse_usage(u);
        }
        for ch in v.get("choices").and_then(|c| c.as_array()).into_iter().flatten() {
            let d = &ch["delta"];
            if let Some(r) = d.get("reasoning_content").and_then(|s| s.as_str())
                && !r.is_empty() {
                    self.reasoning.push_str(r);
                    out.push(StreamEvent::Reasoning { delta: r.into() });
                }
            if let Some(t) = d.get("content").and_then(|s| s.as_str())
                && !t.is_empty() {
                    self.text.push_str(t);
                    out.push(StreamEvent::Text { delta: t.into() });
                }
            for tc in d.get("tool_calls").and_then(|c| c.as_array()).into_iter().flatten() {
                let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
                while self.calls.len() <= idx {
                    self.calls.push(PartialCall::default());
                }
                let pc = &mut self.calls[idx];
                if let Some(id) = tc.get("id").and_then(|s| s.as_str()) {
                    pc.id = id.into();
                }
                if let Some(n) = tc.pointer("/function/name").and_then(|s| s.as_str())
                    && pc.name.is_empty() {
                        pc.name = n.into();
                        out.push(StreamEvent::ToolStart { id: pc.id.clone(), name: pc.name.clone() });
                    }
                if let Some(a) = tc.pointer("/function/arguments").and_then(|s| s.as_str()) {
                    pc.args.push_str(a);
                }
            }
            if let Some(f) = ch.get("finish_reason").and_then(|f| f.as_str()) {
                self.finish = Some(f.into());
                if f == "insufficient_system_resource" {
                    return Err(ProviderError::new(ProviderErrorKind::Overloaded, "insufficient_system_resource"));
                }
            }
        }
        Ok(())
    }
    fn done(&self) -> bool {
        self.done
    }
    fn partial(&self) -> TurnOutput {
        let mut o = self.output(false);
        if o.text.is_empty() { o.raw = Value::Null; }
        o
    }
    fn finish(self: Box<Self>) -> Result<TurnOutput, ProviderError> {
        Ok(self.output(true))
    }
}

pub fn parse_models(v: &Value) -> Vec<ModelCaps> {
    let mut out = Vec::new();
    for m in v.get("data").and_then(|d| d.as_array()).into_iter().flatten() {
        let Some(id) = m.get("id").and_then(|s| s.as_str()) else { continue };
        let mut c = registry::unknown(ProviderId::Deepseek, id);
        c.source = "remote".into();
        c.note = None;
        if let Some(n) = m.get("name").and_then(|s| s.as_str()) {
            c.display_name = n.into();
        }
        c.context_window = m.get("context_window").and_then(|x| x.as_u64()).unwrap_or(0);
        c.max_output = m.get("max_output_tokens").and_then(|x| x.as_u64()).unwrap_or(0);
        c.vision = m
            .get("input_modalities")
            .and_then(|x| x.as_array())
            .is_some_and(|a| a.iter().any(|s| s.as_str() == Some("image")));
        if let Some(levels) = m.pointer("/effort/supported_levels").and_then(|x| x.as_array()) {
            // `none` (thinking off) is documented as always accepted and excluded from the list.
            let mut e: Vec<Effort> = vec![Effort::None];
            e.extend(levels.iter().filter_map(|s| s.as_str().and_then(Effort::parse)));
            e.sort();
            e.dedup();
            c.efforts = e;
            c.default_effort = m.pointer("/effort/default_level").and_then(|s| s.as_str()).and_then(Effort::parse);
        }
        if c.context_window == 0 {
            c.context_window = 128_000;
        }
        if c.max_output == 0 {
            c.max_output = 16_000;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RawReplay;
    use crate::providers::sse::SseParser;
    use crate::providers::testutil::*;

    #[test]
    fn builds_chat_body_with_reasoning_replay() {
        let c = caps(ProviderId::Deepseek, "deepseek-v4-pro");
        let mut h = sample_history(ProviderId::Deepseek, "deepseek-v4-pro");
        h[1].raw = Some(RawReplay { provider: ProviderId::Deepseek, model: "deepseek-v4-pro".into(), data: json!({"reasoning_content": "R"}) });
        let sys = vec!["sys".to_string()];
        let tools = vec![crate::providers::ToolSpec { name: "list_dir", description: "d", parameters: json!({"type":"object"}) }];
        let b = build(&req(ProviderId::Deepseek, &c, &sys, &tools, &h));
        assert_eq!(b.path, "/chat/completions");
        assert_eq!(b.body["messages"][0]["role"], "system");
        assert_eq!(b.body["messages"][2]["reasoning_content"], "R");
        assert_eq!(b.body["messages"][2]["tool_calls"][0]["function"]["arguments"], "{\"path\":\".\"}");
        assert_eq!(b.body["messages"][3]["role"], "tool");
        assert_eq!(b.body["thinking"]["type"], "enabled");
        assert_eq!(b.body["reasoning_effort"], "high");
        assert_eq!(b.body["stream_options"]["include_usage"], true);
        assert!(b.body.get("tool_choice").is_none());
    }

    #[test]
    fn none_effort_disables_thinking() {
        let c = caps(ProviderId::Deepseek, "deepseek-flash");
        let mut r = req(ProviderId::Deepseek, &c, &[], &[], &[]);
        r.effort = Some(Effort::None);
        let b = build(&r);
        assert_eq!(b.body["thinking"]["type"], "disabled");
        assert!(b.body.get("reasoning_effort").is_none());
    }

    #[test]
    fn cache_rate_requires_both_reported_counts() {
        assert_eq!(parse_usage(&json!({"prompt_cache_miss_tokens": 10})).hit_rate(), None);
        assert_eq!(parse_usage(&json!({"prompt_cache_hit_tokens": 10})).hit_rate(), None);
        assert_eq!(parse_usage(&json!({"prompt_cache_hit_tokens": 90, "prompt_cache_miss_tokens": 10})).hit_rate(), Some(0.9));
    }

    #[test]
    fn decodes_stream_with_tool_calls_and_cache_usage() {
        let s = concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"reasoning_content\":\"hmm\"}}]}\n\n",
            ": keep-alive\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"OK\"}}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_0\",\"type\":\"function\",\"function\":{\"name\":\"read_file\",\"arguments\":\"{\\\"pa\"}}]}}]}\n\n",
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"th\\\":\\\"x\\\"}\"}}]},\"finish_reason\":\"tool_calls\"}]}\n\n",
            "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":1000,\"completion_tokens\":20,\"prompt_cache_hit_tokens\":768,\"prompt_cache_miss_tokens\":232,\"completion_tokens_details\":{\"reasoning_tokens\":5}}}\n\n",
            "data: [DONE]\n\n"
        );
        let mut d = Box::new(DeepSeekDecoder::new("deepseek-flash"));
        let mut p = SseParser::new();
        let mut ev = vec![];
        for e in p.feed(s.as_bytes()) {
            d.on_event(e, &mut ev).unwrap();
        }
        assert!(d.done());
        let o = d.finish().unwrap();
        assert_eq!(o.text, "OK");
        assert_eq!(o.tool_calls[0].arguments["path"], "x");
        assert_eq!(o.stop, StopKind::ToolUse);
        assert_eq!(o.raw["reasoning_content"], "hmm");
        assert_eq!(o.usage.cached_input_tokens, 768);
        assert!((o.usage.hit_rate().unwrap() - 0.768).abs() < 1e-9);
    }

    #[test]
    fn parses_remote_model_metadata() {
        let v = json!({"object":"list","data":[{"id":"deepseek-flash","object":"model","owned_by":"deepseek","name":"DeepSeek-V4.1-Flash","context_window":1048576,"max_output_tokens":393216,"input_modalities":["text","image"],"effort":{"supported_levels":["low","high","max"],"default_level":"high"}}]});
        let m = &parse_models(&v)[0];
        assert_eq!(m.efforts, vec![Effort::None, Effort::Low, Effort::High, Effort::Max]);
        assert_eq!(m.default_effort, Some(Effort::High));
        assert!(m.vision);
        assert_eq!(m.context_window, 1048576);
    }
}
