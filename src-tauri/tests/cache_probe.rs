//! Live prompt-cache probe. Spends real tokens, so it is ignored by default.
//!
//!   $env:YCODE_DEEPSEEK_KEY="..."; cargo test --test cache_probe -- --ignored --nocapture
//!
//! Keys: YCODE_OPENAI_KEY, YCODE_DEEPSEEK_KEY, YCODE_ANTHROPIC_KEY (any subset).
//! Optional model overrides: YCODE_OPENAI_MODEL, YCODE_DEEPSEEK_MODEL, YCODE_ANTHROPIC_MODEL.
//! Results are written to ../cache-results/<provider>-<timestamp>.json.

use serde_json::json;
use ycode_lib::agent::prompts::system_blocks;
use ycode_lib::model::{Control, Effort, Message, Mode, ProviderId, Role, Usage, now_ms};
use ycode_lib::providers::{self, Cancel, Endpoint, TurnRequest, TurnResult, registry};
use ycode_lib::skills::SkillStore;

struct Step {
    name: &'static str,
    expect: &'static str,
    usage: Option<Usage>,
    error: Option<String>,
}

/// ~4k tokens of stable filler so every prompt clears the providers' minimum
/// cacheable length (OpenAI 1024, Claude 512, DeepSeek undocumented).
fn filler() -> String {
    let para = "Ycode cache probe reference text. This paragraph is identical in every request so the provider can reuse the cached prefix. ";
    format!("# Reference\n{}", para.repeat(160))
}

fn user(text: &str) -> Message {
    let mut m = Message::new(Role::User);
    m.text = text.into();
    m
}

#[allow(clippy::too_many_arguments)]
async fn turn(http: &reqwest::Client, ep: &Endpoint, p: ProviderId, caps: &registry::ModelCaps, system: &[String], mode: Mode, h: &[Message], effort: Option<Effort>) -> Result<(Usage, Message), String> {
    let tools = ycode_lib::tools::specs(mode);
    let req = TurnRequest {
        provider: p,
        model: &caps.id,
        caps,
        system,
        tools: &tools,
        history: h,
        effort,
        cache_key: "ycode-cache-probe",
        max_output: 512,
        fallbacks: false,
    };
    let mut sink = |_e: providers::StreamEvent| {};
    match providers::run_turn(http, ep, &req, &Cancel::default(), &mut sink).await {
        Ok(TurnResult::Done(o)) => {
            let mut a = Message::new(Role::Assistant);
            a.text = o.text;
            a.provider = Some(p);
            a.model = Some(caps.id.clone());
            a.effort = effort;
            a.raw = (!o.raw.is_null()).then(|| ycode_lib::model::RawReplay { provider: p, model: caps.id.clone(), data: o.raw });
            Ok((o.usage, a))
        }
        Ok(TurnResult::Stopped(_)) => Err("stopped".into()),
        Ok(TurnResult::Steered(_)) => Err("steered".into()),
        Ok(TurnResult::Interrupted(_, e)) => Err(format!("{e}")),
        Err(e) => Err(format!("{e}")),
    }
}

async fn probe(p: ProviderId, key: String, model: String) {
    let http = providers::http_client();
    let ep = Endpoint { base_url: p.default_base_url().into(), api_key: key };
    let caps = registry::builtin().into_iter().find(|c| c.provider == p && c.id == model).unwrap_or_else(|| registry::unknown(p, &model));
    let skills = SkillStore::empty();
    let mut sys = system_blocks(Mode::Office, &skills, None);
    sys.push(filler());
    let e0 = caps.default_effort;
    let mut steps: Vec<Step> = Vec::new();
    let mut h = vec![user("Reply with one short sentence: what is a prompt cache?")];

    macro_rules! run {
        ($name:expr, $expect:expr, $sys:expr, $mode:expr, $hist:expr, $eff:expr) => {{
            let r = turn(&http, &ep, p, &caps, $sys, $mode, $hist, $eff).await;
            match r {
                Ok((u, a)) => {
                    steps.push(Step { name: $name, expect: $expect, usage: Some(u), error: None });
                    Some(a)
                }
                Err(e) => {
                    steps.push(Step { name: $name, expect: $expect, usage: None, error: Some(e) });
                    None
                }
            }
        }};
    }

    // 1. Same system + same first message twice.
    run!("1a_first_call", "miss (cold) or write", &sys, Mode::Office, &h, e0);
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let a = run!("1b_identical_repeat", "hit on system+tools prefix", &sys, Mode::Office, &h, e0);
    // 2. Multi-turn append.
    if let Some(a) = a {
        h.push(a);
    }
    h.push(user("Now one sentence on why prefix order matters."));
    let a = run!("2_append_turn", "hit up to previous turn", &sys, Mode::Office, &h, e0);
    if let Some(a) = a {
        h.push(a);
    }
    // 3. New dynamic user content at the end.
    h.push(user("Dynamic content: the current request id is 7f3a. Acknowledge briefly."));
    let a = run!("3_dynamic_tail", "hit up to previous turn; tail uncached", &sys, Mode::Office, &h, e0);
    if let Some(a) = a {
        h.push(a);
    }
    // 4. Effort change (in-band where supported).
    let e1 = caps.efforts.iter().copied().rev().find(|e| Some(*e) != e0 && *e != Effort::None);
    if let Some(e1) = e1 {
        let in_band = caps.effort_update != registry::EffortUpdate::Param;
        let mut ctl = Message::new(Role::Control);
        ctl.control = Some(Control::Effort { provider: p, model: caps.id.clone(), effort: e1, in_band });
        h.push(ctl);
        h.push(user("One more sentence, please."));
        let req_effort = if in_band { e0 } else { Some(e1) };
        let a = run!("4_effort_change", if in_band { "hit (in-band update keeps prefix)" } else { "possible miss (parameter change)" }, &sys, Mode::Office, &h, req_effort);
        if let Some(a) = a {
            h.push(a);
        }
    }
    // 5. Skill index added to the system prompt.
    let mut sys_skill = sys.clone();
    sys_skill.insert(2, "# Skills\n- probe-skill: Example skill used by the cache probe.".into());
    h.push(user("Short answer: ok?"));
    run!("5_skill_index_changed", "miss after system change (tools prefix may still hit)", &sys_skill, Mode::Office, &h, e0);
    // 6. Mode switch (system + tools change).
    let mut sys_code = system_blocks(Mode::Coding, &skills, None);
    sys_code.push(filler());
    run!("6_mode_switch", "miss (tools and system changed)", &sys_code, Mode::Coding, &h, e0);
    // 7. Repeat after mode switch → should hit again.
    run!("7_repeat_after_switch", "hit (new prefix now cached)", &sys_code, Mode::Coding, &h, e0);

    let rows: Vec<_> = steps
        .iter()
        .map(|s| {
            json!({
                "step": s.name,
                "expected": s.expect,
                "usage": s.usage,
                "hit_rate": s.usage.as_ref().and_then(|u| u.hit_rate()),
                "error": s.error,
            })
        })
        .collect();
    for r in &rows {
        println!("{} {}", p.as_str(), r);
    }
    let out = json!({ "provider": p.as_str(), "model": caps.id, "ran_at_ms": now_ms(), "steps": rows });
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../cache-results");
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join(format!("{}-{}.json", p.as_str(), now_ms())), serde_json::to_string_pretty(&out).unwrap());
}

#[tokio::test]
#[ignore]
async fn live_cache_probe() {
    let cfg = [
        (ProviderId::Openai, "YCODE_OPENAI_KEY", "YCODE_OPENAI_MODEL", "gpt-6.1-terra"),
        (ProviderId::Deepseek, "YCODE_DEEPSEEK_KEY", "YCODE_DEEPSEEK_MODEL", "deepseek-flash"),
        (ProviderId::Anthropic, "YCODE_ANTHROPIC_KEY", "YCODE_ANTHROPIC_MODEL", "claude-haiku-5-5"),
    ];
    let mut ran = 0;
    for (p, kv, mv, default_model) in cfg {
        if let Ok(key) = std::env::var(kv) {
            let model = std::env::var(mv).unwrap_or_else(|_| default_model.to_string());
            probe(p, key, model).await;
            ran += 1;
        }
    }
    if ran == 0 {
        eprintln!("No YCODE_*_KEY set; nothing probed.");
    }
}
