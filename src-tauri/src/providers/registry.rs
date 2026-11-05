//! Built-in model capability table (from MODEL_RESEARCH.md, 2026-11-05) merged with
//! what each vendor's list-models endpoint reports at runtime and with user
//! overrides. Unknown models get *no* effort control (we never guess API params
//! from a model name) — the user can add an override in Settings.

use crate::model::{Effort, Mode, ProviderId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EffortUpdate {
    /// Changing effort means changing a request parameter (may cost a cache miss).
    Param,
    /// OpenAI GPT-6: `configuration_update` input item.
    OpenaiItem,
    /// Anthropic beta `mid-conversation-output-config-2026-07-01`.
    AnthropicSystemMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelCaps {
    pub provider: ProviderId,
    pub id: String,
    pub display_name: String,
    /// Effort values accepted by the API, ascending. Empty → effort not adjustable.
    pub efforts: Vec<Effort>,
    pub default_effort: Option<Effort>,
    pub effort_update: EffortUpdate,
    pub tools: bool,
    pub vision: bool,
    pub context_window: u64,
    pub max_output: u64,
    /// Minimum prefix length (tokens) for provider caching; 0 = unknown/none.
    pub min_cache_tokens: u64,
    pub recommended: Vec<Mode>,
    /// Anthropic: send `thinking: {type: adaptive}`.
    pub adaptive_thinking: bool,
    /// Anthropic: opt into server-side refusal fallbacks (`fallbacks: "default"`).
    pub server_fallbacks: bool,
    /// Where the data came from: builtin | remote | override | unknown
    pub source: String,
    /// Returned by the vendor's /models for the current key (None = not refreshed yet).
    pub available: Option<bool>,
    pub note: Option<String>,
}

const E5: [Effort; 5] = [Effort::Low, Effort::Medium, Effort::High, Effort::Xhigh, Effort::Max];
const E6: [Effort; 6] = [Effort::None, Effort::Low, Effort::Medium, Effort::High, Effort::Xhigh, Effort::Max];
const DS: [Effort; 4] = [Effort::None, Effort::Low, Effort::High, Effort::Max];

#[allow(clippy::too_many_arguments)]
fn caps(
    provider: ProviderId,
    id: &str,
    name: &str,
    efforts: &[Effort],
    default_effort: Effort,
    effort_update: EffortUpdate,
    vision: bool,
    context: u64,
    max_out: u64,
    min_cache: u64,
    recommended: &[Mode],
) -> ModelCaps {
    ModelCaps {
        provider,
        id: id.into(),
        display_name: name.into(),
        efforts: efforts.to_vec(),
        default_effort: Some(default_effort),
        effort_update,
        tools: true,
        vision,
        context_window: context,
        max_output: max_out,
        min_cache_tokens: min_cache,
        recommended: recommended.to_vec(),
        adaptive_thinking: provider == ProviderId::Anthropic,
        server_fallbacks: false,
        source: "builtin".into(),
        available: None,
        note: None,
    }
}

pub fn builtin() -> Vec<ModelCaps> {
    use EffortUpdate::*;
    use Mode::*;
    use ProviderId::*;
    let mut v = vec![
        caps(Openai, "gpt-6-astra", "GPT-6 Astra", &E5, Effort::Medium, OpenaiItem, true, 1_050_000, 128_000, 1024, &[Coding]),
        caps(Openai, "gpt-6.1-sol", "GPT-6.1 Sol", &E5, Effort::Medium, OpenaiItem, true, 1_050_000, 128_000, 1024, &[Coding, Office]),
        caps(Openai, "gpt-6.1-terra", "GPT-6.1 Terra", &E6, Effort::Medium, OpenaiItem, true, 1_050_000, 128_000, 1024, &[Office]),
        caps(Openai, "gpt-6-luna", "GPT-6 Luna", &E6, Effort::Medium, OpenaiItem, true, 1_050_000, 128_000, 1024, &[]),
        caps(Deepseek, "deepseek-v4-pro", "DeepSeek V4 Pro", &DS, Effort::High, Param, false, 1_048_576, 393_216, 0, &[Coding]),
        caps(Deepseek, "deepseek-flash", "DeepSeek V4.1 Flash", &DS, Effort::High, Param, true, 1_048_576, 393_216, 0, &[Office]),
        caps(Anthropic, "claude-opus-5-5", "Claude Opus 5.5", &E5, Effort::Medium, AnthropicSystemMessage, true, 1_000_000, 128_000, 512, &[Coding]),
        caps(Anthropic, "claude-sonnet-5-5", "Claude Sonnet 5.5", &E5, Effort::High, AnthropicSystemMessage, true, 1_000_000, 128_000, 512, &[Office]),
        caps(Anthropic, "claude-fable-5-1", "Claude Fable 5.1", &E5, Effort::High, AnthropicSystemMessage, true, 1_000_000, 128_000, 512, &[]),
        caps(Anthropic, "claude-haiku-5-5", "Claude Haiku 5.5", &E5, Effort::Medium, AnthropicSystemMessage, true, 1_000_000, 128_000, 512, &[]),
    ];
    for c in v.iter_mut() {
        if matches!(c.id.as_str(), "claude-opus-5-5" | "claude-sonnet-5-5" | "claude-fable-5-1") {
            c.server_fallbacks = true;
        }
        if c.provider == Deepseek {
            c.note = Some("effort 'none' disables thinking mode".into());
        }
    }
    v
}

/// Capabilities for a model the built-in table doesn't know. Tool calling is assumed
/// (all current chat models of the three vendors support it); effort is disabled.
pub fn unknown(provider: ProviderId, id: &str) -> ModelCaps {
    ModelCaps {
        provider,
        id: id.into(),
        display_name: id.into(),
        efforts: vec![],
        default_effort: None,
        effort_update: EffortUpdate::Param,
        tools: true,
        vision: false,
        context_window: 128_000,
        max_output: 16_000,
        min_cache_tokens: 0,
        recommended: vec![],
        adaptive_thinking: false,
        server_fallbacks: false,
        source: "unknown".into(),
        available: None,
        note: Some("capabilities unknown: effort control disabled".into()),
    }
}

/// Merge capability data reported by a vendor's /models into the table.
pub fn merge_remote(table: &mut Vec<ModelCaps>, provider: ProviderId, remote: Vec<ModelCaps>) {
    for c in table.iter_mut().filter(|c| c.provider == provider) {
        c.available = Some(false);
    }
    for r in remote {
        if let Some(existing) = table.iter_mut().find(|c| c.provider == provider && c.id == r.id) {
            existing.available = Some(true);
            // Remote metadata wins where it is authoritative (limits, effort levels).
            if r.context_window > 0 {
                existing.context_window = r.context_window;
            }
            if r.max_output > 0 {
                existing.max_output = r.max_output;
            }
            if !r.efforts.is_empty() {
                existing.efforts = r.efforts.clone();
                if r.default_effort.is_some() {
                    existing.default_effort = r.default_effort;
                }
            }
            if r.display_name != r.id {
                existing.display_name = r.display_name.clone();
            }
        } else {
            let mut r = r;
            r.available = Some(true);
            table.push(r);
        }
    }
}

pub fn clamp_effort(caps: &ModelCaps, wanted: Option<Effort>) -> Option<Effort> {
    if caps.efforts.is_empty() {
        return None;
    }
    match wanted {
        Some(e) if caps.efforts.contains(&e) => Some(e),
        _ => caps.default_effort.filter(|d| caps.efforts.contains(d)).or_else(|| caps.efforts.first().copied()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_ids_are_unique_and_have_valid_defaults() {
        let b = builtin();
        let mut ids: Vec<_> = b.iter().map(|c| (c.provider, c.id.clone())).collect();
        ids.sort_by(|a, b| a.1.cmp(&b.1));
        ids.dedup();
        assert_eq!(ids.len(), b.len());
        for c in &b {
            let d = c.default_effort.unwrap();
            assert!(c.efforts.contains(&d), "{} default not in list", c.id);
        }
    }

    #[test]
    fn clamp_rejects_unsupported_levels() {
        let b = builtin();
        let ds = b.iter().find(|c| c.id == "deepseek-flash").unwrap();
        assert_eq!(clamp_effort(ds, Some(Effort::Medium)), Some(Effort::High));
        assert_eq!(clamp_effort(ds, Some(Effort::Max)), Some(Effort::Max));
        let astra = b.iter().find(|c| c.id == "gpt-6-astra").unwrap();
        assert_eq!(clamp_effort(astra, Some(Effort::None)), Some(Effort::Medium));
        let u = unknown(ProviderId::Openai, "x");
        assert_eq!(clamp_effort(&u, Some(Effort::High)), None);
    }

    #[test]
    fn merge_marks_availability_and_keeps_manual() {
        let mut t = builtin();
        let mut r = unknown(ProviderId::Deepseek, "deepseek-flash");
        r.efforts = vec![Effort::Low, Effort::High];
        merge_remote(&mut t, ProviderId::Deepseek, vec![r, unknown(ProviderId::Deepseek, "deepseek-new")]);
        let f = t.iter().find(|c| c.id == "deepseek-flash").unwrap();
        assert_eq!(f.available, Some(true));
        assert_eq!(f.efforts, vec![Effort::Low, Effort::High]);
        assert_eq!(t.iter().find(|c| c.id == "deepseek-v4-pro").unwrap().available, Some(false));
        assert!(t.iter().any(|c| c.id == "deepseek-new"));
    }
}
