//! OpenAI Responses API adapter (`POST /responses`, stateless `store:false` replay).

use super::registry::{self, ModelCaps};
use super::sse::SseEvent;
use super::{BuiltRequest, Decoder, StopKind, StreamEvent, TurnOutput, TurnRequest, parse_args};
use crate::error::{ProviderError, ProviderErrorKind};
use crate::model::{Control, Effort, ProviderId, Role, ToolCall, Usage};
use serde_json::{Value, json};

pub fn build(req: &TurnRequest) -> BuiltRequest {
    let mut input: Vec<Value> = Vec::new();
    for m in req.history {
        match m.role {
            Role::User => input.push(json!({ "role": "user", "content": m.user_content_text() })),
            Role::Assistant => {
                if let Some(Value::Array(items)) = m.raw_for(ProviderId::Openai, req.model) {
                    for it in items {
                        input.push(clean_item(it));
                    }
                } else {
                    if !m.text.is_empty() {
                        input.push(json!({
                            "role": "assistant",
                            "content": [{ "type": "output_text", "text": m.text }]
                        }));
                    }
                    for c in &m.tool_calls {
                        input.push(json!({
                            "type": "function_call",
                            "call_id": c.id,
                            "name": c.name,
                            "arguments": c.arguments.to_string(),
                        }));
                    }
                }
            }
            Role::Tool => {
                for r in &m.tool_results {
                    input.push(json!({ "type": "function_call_output", "call_id": r.call_id, "output": r.content }));
                }
            }
            Role::Control => {
                if let Some(Control::Effort { provider: ProviderId::Openai, model, effort, in_band: true }) = &m.control
                    && model == req.model {
                        input.push(json!({ "type": "configuration_update", "reasoning": { "effort": effort.as_str() } }));
                    }
            }
        }
    }
    let tools: Vec<Value> = req
        .tools
        .iter()
        .map(|t| json!({ "type": "function", "name": t.name, "description": t.description, "parameters": t.parameters, "strict": false }))
        .collect();
    let mut body = json!({
        "model": req.model,
        "store": false,
        "stream": true,
        "instructions": req.system.join("\n\n"),
        "input": input,
        "max_output_tokens": req.max_output,
        "prompt_cache_key": req.cache_key,
    });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
        body["tool_choice"] = json!("auto");
        body["parallel_tool_calls"] = json!(true);
    }
    if let Some(e) = req.effort {
        body["reasoning"] = if e == Effort::None { json!({ "effort": "none" }) } else { json!({ "effort": e.as_str(), "summary": "auto" }) };
    }
    BuiltRequest { path: "/responses".into(), headers: vec![], body }
}

/// Output items are replayed without server-side ids/status (stateless mode).
fn clean_item(it: &Value) -> Value {
    let mut it = it.clone();
    if let Some(o) = it.as_object_mut() {
        o.remove("id");
        o.remove("status");
    }
    it
}

pub struct OpenAiDecoder {
    items: Vec<(u64, Value)>,
    text: String,
    reasoning: String,
    usage: Usage,
    done: bool,
    incomplete: Option<String>,
}

impl OpenAiDecoder {
    pub fn new(_model: &str) -> Self {
        Self { items: vec![], text: String::new(), reasoning: String::new(), usage: Usage::default(), done: false, incomplete: None }
    }

    fn assemble(&self, items: &[Value]) -> TurnOutput {
        let mut text = String::new();
        let mut refusal = false;
        let mut calls = Vec::new();
        for it in items {
            match it.get("type").and_then(|t| t.as_str()) {
                Some("message") => {
                    for c in it.get("content").and_then(|c| c.as_array()).into_iter().flatten() {
                        match c.get("type").and_then(|t| t.as_str()) {
                            Some("output_text") => text.push_str(c.get("text").and_then(|t| t.as_str()).unwrap_or("")),
                            Some("refusal") => {
                                refusal = true;
                                text.push_str(c.get("refusal").and_then(|t| t.as_str()).unwrap_or(""));
                            }
                            _ => {}
                        }
                    }
                }
                Some("function_call") => calls.push(ToolCall {
                    id: it.get("call_id").and_then(|s| s.as_str()).unwrap_or_default().to_string(),
                    name: it.get("name").and_then(|s| s.as_str()).unwrap_or_default().to_string(),
                    arguments: parse_args(it.get("arguments").and_then(|s| s.as_str()).unwrap_or("")),
                }),
                _ => {}
            }
        }
        if text.is_empty() {
            text = self.text.clone();
        }
        let stop = if refusal {
            StopKind::Refusal
        } else if !calls.is_empty() {
            StopKind::ToolUse
        } else if let Some(r) = &self.incomplete {
            if r == "max_output_tokens" { StopKind::MaxTokens } else { StopKind::Other(r.clone()) }
        } else {
            StopKind::End
        };
        TurnOutput {
            text,
            reasoning: if self.reasoning.is_empty() { None } else { Some(self.reasoning.clone()) },
            tool_calls: calls,
            raw: Value::Array(items.to_vec()),
            usage: self.usage.clone(),
            stop,
        }
    }
}

pub fn parse_usage(u: &Value) -> Usage {
    let g = |p: &str| u.pointer(p).and_then(|v| v.as_u64());
    let input = g("/input_tokens").unwrap_or(0);
    Usage {
        input_tokens: input,
        output_tokens: g("/output_tokens").unwrap_or(0),
        cached_input_tokens: g("/input_tokens_details/cached_tokens").unwrap_or(0),
        cache_write_tokens: g("/input_tokens_details/cache_write_tokens"),
        reasoning_tokens: g("/output_tokens_details/reasoning_tokens"),
        eligible_input_tokens: Some(input),
    }
}

impl Decoder for OpenAiDecoder {
    fn on_event(&mut self, ev: SseEvent, out: &mut Vec<StreamEvent>) -> Result<(), ProviderError> {
        if ev.data.trim() == "[DONE]" {
            return Ok(());
        }
        let v: Value = match serde_json::from_str(&ev.data) {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };
        let ty = v.get("type").and_then(|t| t.as_str()).or(ev.event.as_deref()).unwrap_or("");
        match ty {
            "response.output_text.delta" => {
                let d = v.get("delta").and_then(|d| d.as_str()).unwrap_or("");
                self.text.push_str(d);
                out.push(StreamEvent::Text { delta: d.to_string() });
            }
            "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => {
                let d = v.get("delta").and_then(|d| d.as_str()).unwrap_or("");
                self.reasoning.push_str(d);
                out.push(StreamEvent::Reasoning { delta: d.to_string() });
            }
            "response.reasoning_summary_part.done" => {
                self.reasoning.push_str("\n\n");
                out.push(StreamEvent::Reasoning { delta: "\n\n".into() });
            }
            "response.output_item.added" => {
                if let Some(item) = v.get("item")
                    && item.get("type").and_then(|t| t.as_str()) == Some("function_call") {
                        out.push(StreamEvent::ToolStart {
                            id: item.get("call_id").and_then(|s| s.as_str()).unwrap_or_default().into(),
                            name: item.get("name").and_then(|s| s.as_str()).unwrap_or_default().into(),
                        });
                    }
            }
            "response.output_item.done" => {
                let idx = v.get("output_index").and_then(|i| i.as_u64()).unwrap_or(self.items.len() as u64);
                if let Some(item) = v.get("item") {
                    self.items.push((idx, item.clone()));
                }
            }
            "response.completed" | "response.incomplete" => {
                if let Some(u) = v.pointer("/response/usage") {
                    self.usage = parse_usage(u);
                }
                if ty == "response.incomplete" {
                    self.incomplete = Some(
                        v.pointer("/response/incomplete_details/reason").and_then(|r| r.as_str()).unwrap_or("incomplete").to_string(),
                    );
                }
                // Prefer the authoritative final output list when present.
                if let Some(Value::Array(output)) = v.pointer("/response/output")
                    && !output.is_empty() {
                        self.items = output.iter().cloned().enumerate().map(|(i, it)| (i as u64, it)).collect();
                    }
                self.done = true;
            }
            "response.failed" => {
                let msg = v.pointer("/response/error/message").and_then(|m| m.as_str()).unwrap_or("response failed");
                let code = v.pointer("/response/error/code").and_then(|m| m.as_str()).unwrap_or("");
                return Err(classify_stream_error(code, msg));
            }
            "error" => {
                let msg = v.get("message").or_else(|| v.pointer("/error/message")).and_then(|m| m.as_str()).unwrap_or("stream error");
                let code = v.get("code").or_else(|| v.pointer("/error/code")).and_then(|m| m.as_str()).unwrap_or("");
                return Err(classify_stream_error(code, msg));
            }
            _ => {}
        }
        Ok(())
    }

    fn done(&self) -> bool {
        self.done
    }

    fn partial(&self) -> TurnOutput {
        // Stopped mid-stream: keep only the visible text; incomplete tool calls and
        // reasoning items are dropped so the history stays valid.
        let mut o = self.assemble(&[]);
        o.tool_calls.clear();
        o.raw = Value::Null;
        o.stop = StopKind::Other("stopped".into());
        o
    }

    fn finish(mut self: Box<Self>) -> Result<TurnOutput, ProviderError> {
        self.items.sort_by_key(|(i, _)| *i);
        let items: Vec<Value> = self.items.iter().map(|(_, v)| v.clone()).collect();
        Ok(self.assemble(&items))
    }
}

fn classify_stream_error(code: &str, msg: &str) -> ProviderError {
    let kind = match code {
        "rate_limit_exceeded" => ProviderErrorKind::RateLimited,
        "insufficient_quota" => ProviderErrorKind::InsufficientBalance,
        "server_error" => ProviderErrorKind::Server,
        "model_not_found" => ProviderErrorKind::ModelNotFound,
        _ => ProviderErrorKind::Unknown,
    };
    ProviderError::new(kind, msg)
}

const EXCLUDE: [&str; 13] =
    ["image", "realtime", "audio", "transcribe", "tts", "embedding", "whisper", "moderation", "live", "dall-e", "search", "cyber", "rosalind"];

pub fn parse_models(v: &Value) -> Vec<ModelCaps> {
    let mut out = Vec::new();
    for m in v.get("data").and_then(|d| d.as_array()).into_iter().flatten() {
        let Some(id) = m.get("id").and_then(|s| s.as_str()) else { continue };
        let chat_like = id.starts_with("gpt-") || (id.starts_with('o') && id.chars().nth(1).is_some_and(|c| c.is_ascii_digit()));
        if !chat_like || EXCLUDE.iter().any(|x| id.contains(x)) {
            continue;
        }
        out.push(registry::unknown(ProviderId::Openai, id));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Message, Role};
    use crate::providers::testutil::*;
    use crate::providers::{ToolSpec, sse::SseParser};

    fn tools() -> Vec<ToolSpec> {
        vec![ToolSpec { name: "list_dir", description: "List", parameters: json!({"type":"object","properties":{}}) }]
    }

    #[test]
    fn builds_responses_body() {
        let c = caps(ProviderId::Openai, "gpt-6-astra");
        let sys = vec!["base".to_string(), "mode".to_string()];
        let t = tools();
        let h = sample_history(ProviderId::Openai, "gpt-6-astra");
        let b = build(&req(ProviderId::Openai, &c, &sys, &t, &h));
        assert_eq!(b.path, "/responses");
        assert_eq!(b.body["store"], false);
        assert_eq!(b.body["instructions"], "base\n\nmode");
        assert_eq!(b.body["reasoning"]["effort"], "medium");
        assert_eq!(b.body["input"][1]["role"], "assistant");
        assert_eq!(b.body["input"][2]["type"], "function_call");
        assert_eq!(b.body["input"][2]["arguments"], "{\"path\":\".\"}");
        assert_eq!(b.body["input"][3]["type"], "function_call_output");
        assert_eq!(b.body["tools"][0]["name"], "list_dir");
        assert!(b.body.get("temperature").is_none());
    }

    #[test]
    fn effort_change_is_in_band_and_keeps_prefix() {
        let c = caps(ProviderId::Openai, "gpt-6-astra");
        let sys = vec!["base".to_string()];
        let t = tools();
        let h1 = sample_history(ProviderId::Openai, "gpt-6-astra");
        let b1 = build(&req(ProviderId::Openai, &c, &sys, &t, &h1));
        let mut h2 = h1.clone();
        let mut ctl = Message::new(Role::Control);
        ctl.control = Some(Control::Effort { provider: ProviderId::Openai, model: "gpt-6-astra".into(), effort: Effort::High, in_band: true });
        h2.push(ctl);
        let mut u = Message::new(Role::User);
        u.text = "next".into();
        h2.push(u);
        let b2 = build(&req(ProviderId::Openai, &c, &sys, &t, &h2));
        assert!(messages_prefix_stable(&b1.body, &b2.body, "input"));
        assert_eq!(b2.body["input"][4]["type"], "configuration_update");
        assert_eq!(b2.body["input"][4]["reasoning"]["effort"], "high");
        // Request-level effort unchanged → instructions/tools/reasoning prefix identical.
        assert_eq!(b1.body["reasoning"], b2.body["reasoning"]);
        assert_eq!(b1.body["tools"], b2.body["tools"]);
    }

    #[test]
    fn replays_raw_items_without_ids() {
        let c = caps(ProviderId::Openai, "gpt-6-astra");
        let mut h = sample_history(ProviderId::Openai, "gpt-6-astra");
        h[1].raw = Some(crate::model::RawReplay {
            provider: ProviderId::Openai,
            model: "gpt-6-astra".into(),
            data: json!([{"id":"rs_1","type":"reasoning","summary":[],"encrypted_content":"ENC"},{"id":"fc_1","type":"function_call","call_id":"call_1","name":"list_dir","arguments":"{}","status":"completed"}]),
        });
        let b = build(&req(ProviderId::Openai, &c, &[], &[], &h));
        assert_eq!(b.body["input"][1]["type"], "reasoning");
        assert_eq!(b.body["input"][1]["encrypted_content"], "ENC");
        assert!(b.body["input"][1].get("id").is_none());
        assert!(b.body["input"][2].get("status").is_none());
        assert!(b.body.get("tools").is_none());
    }

    const STREAM: &str = concat!(
        "event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"r1\"}}\n\n",
        "event: response.reasoning_summary_text.delta\ndata: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"thinking\"}\n\n",
        "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"Hel\"}\n\n",
        "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"lo\"}\n\n",
        "event: response.output_item.added\ndata: {\"type\":\"response.output_item.added\",\"output_index\":2,\"item\":{\"type\":\"function_call\",\"call_id\":\"c9\",\"name\":\"read_file\",\"arguments\":\"\"}}\n\n",
        "event: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"type\":\"reasoning\",\"id\":\"rs\",\"summary\":[],\"encrypted_content\":\"E\"}}\n\n",
        "event: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"output_index\":1,\"item\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"Hello\"}]}}\n\n",
        "event: response.output_item.done\ndata: {\"type\":\"response.output_item.done\",\"output_index\":2,\"item\":{\"type\":\"function_call\",\"call_id\":\"c9\",\"name\":\"read_file\",\"arguments\":\"{\\\"path\\\":\\\"a.txt\\\"}\"}}\n\n",
        "event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":2000,\"input_tokens_details\":{\"cached_tokens\":1536,\"cache_write_tokens\":0},\"output_tokens\":50,\"output_tokens_details\":{\"reasoning_tokens\":20}}}}\n\n",
    );

    #[test]
    fn decodes_stream() {
        let mut d = Box::new(OpenAiDecoder::new("gpt-6-astra"));
        let mut p = SseParser::new();
        let mut ev = vec![];
        for e in p.feed(STREAM.as_bytes()) {
            d.on_event(e, &mut ev).unwrap();
        }
        assert!(d.done());
        let o = d.finish().unwrap();
        assert_eq!(o.text, "Hello");
        assert_eq!(o.reasoning.as_deref(), Some("thinking"));
        assert_eq!(o.tool_calls[0].name, "read_file");
        assert_eq!(o.tool_calls[0].arguments["path"], "a.txt");
        assert_eq!(o.stop, StopKind::ToolUse);
        assert_eq!(o.usage.cached_input_tokens, 1536);
        assert_eq!(o.usage.reasoning_tokens, Some(20));
        assert!((o.usage.hit_rate().unwrap() - 0.768).abs() < 1e-9);
        assert_eq!(o.raw.as_array().unwrap().len(), 3);
        assert!(ev.iter().any(|e| matches!(e, StreamEvent::ToolStart { name, .. } if name == "read_file")));
    }

    #[test]
    fn failed_event_is_error() {
        let mut d = OpenAiDecoder::new("m");
        let r = d.on_event(
            SseEvent { event: None, data: r#"{"type":"response.failed","response":{"error":{"code":"rate_limit_exceeded","message":"slow down"}}}"#.into() },
            &mut vec![],
        );
        assert_eq!(r.unwrap_err().kind, ProviderErrorKind::RateLimited);
    }

    #[test]
    fn model_list_filters_non_chat() {
        let v = json!({"data":[{"id":"gpt-6-astra"},{"id":"gpt-image-2.6-flare"},{"id":"gpt-realtime-2"},{"id":"text-embedding-3"},{"id":"o3"}]});
        let ids: Vec<_> = parse_models(&v).into_iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["gpt-6-astra", "o3"]);
    }
}
