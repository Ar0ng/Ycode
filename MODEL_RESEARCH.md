# Ycode — Model & API Research

**Research date:** 2026-11-05
**Method:** Official vendor docs fetched live on the date above (see "Sources"). Nothing in this file
was verified with a real API key — the "Verified with live key" column is `No` everywhere until a key
owner runs the checks in `docs/TESTING.md`.

The app never relies on this table alone: every provider has a **Refresh models** action that calls the
vendor's list-models endpoint at runtime, and every model field accepts a hand-typed ID. The built-in
capability table lives in `src-tauri/src/providers/registry.rs` and is overridable at runtime
(`model_overrides.json` in the app data folder) so new models don't require a rebuild.

---

## 1. OpenAI (GPT)

| | |
|---|---|
| Protocol used by Ycode | **Responses API** `POST /v1/responses` (stateless: `store: false`, full item replay) |
| Base URL | `https://api.openai.com/v1` |
| Auth | `Authorization: Bearer <key>` |
| List models | `GET /v1/models` → `{data:[{id, created, owned_by, shutdown_date?}]}` — **no capability metadata** |

### Flagship models (docs "Models" page, 2026-11-05)

| Model ID | Reasoning effort values | Context | Max out | Tools | Image in | Positioning (vendor wording, summarised) | Ycode use |
|---|---|---|---|---|---|---|---|
| `gpt-6-astra` | low, medium, high, xhigh, max (rejects `none`) | 1.05M | 128K | functions | yes | most capable; complex reasoning + coding | **Coding default** |
| `gpt-6.1-sol` | low, medium, high, xhigh, max (no `none`/`minimal`) | 1.05M | 128K | functions | yes | near-Astra at lower cost | Coding / Office |
| `gpt-6.1-terra` | none, low, medium, high, xhigh, max | 1.05M | 128K | functions | yes | fast; clear natural writing | **Office default** |
| `gpt-6-luna` | none, low, medium, high, xhigh, max | 1.05M | 128K | functions | yes | most efficient, high volume | light Office |

Default effort when omitted: `medium` (stated for GPT-5.6+ / GPT-6.1 Sol). Ycode sends an explicit value.
Specialised models (image, realtime, transcription, `gpt-5.6-cyber`) are out of scope and filtered from
the picker.

### Reasoning effort
* Request field: `reasoning: { "effort": "<value>" }`. Allowed universe: `none, minimal, low, medium, high, xhigh, max`; each model accepts a subset → Ycode only shows that subset.
* **Mid-conversation change without breaking the cache (GPT-6 family):** keep the request-level
  `reasoning.effort` as it was at conversation start and append an input item
  `{"type":"configuration_update","reasoning":{"effort":"high"}}` before the next user message. Applies
  until overridden. Docs: do not combine with automatic compaction/truncation. Ycode does exactly this
  for models flagged `effort_update_item`.
* With `store:false` reasoning items carry `encrypted_content`; Ycode replays every output item
  (reasoning, message, function_call) plus `function_call_output` items.

### Tool calling (Responses)
* Tool: `{"type":"function","name","description","parameters","strict"}`.
* Output item: `{"type":"function_call","id","call_id","name","arguments"(JSON string)}`.
* Result: `{"type":"function_call_output","call_id","output"}`.
* `tool_choice`: `auto | required | none | {type:function,name} | {type:allowed_tools,...}`; `parallel_tool_calls` boolean.

### Prompt caching
* Automatic. On GPT-5.6+: `prompt_cache_options.mode` = `implicit` (default; breakpoint at end of latest
  message) or `explicit`; TTL via `prompt_cache_options.ttl` = `"30m"` (default) / `"12h"` (Responses only).
  Older models: `prompt_cache_retention` (`in_memory`/`24h`) and `prompt_cache_key` for routing.
* Minimum cacheable prefix: 1,024 tokens (GPT-5.6+).
* Usage: `usage.input_tokens_details.cached_tokens` (read), `usage.input_tokens_details.cache_write_tokens`.
* Pricing: reads 0.1× input (0.05× for Sol/Terra), 30-min writes 1.25×, 12-h writes 2.5×.
* Invalidators: tool names/descriptions/schemas/order, `reasoning.effort` changes (use `configuration_update`),
  `text.format`, `parallel_tool_calls`, `text.verbosity`, compaction, editing earlier messages.
* **Ycode policy:** implicit mode (no extra params); stable tool list for the whole conversation;
  `prompt_cache_key` = stable per-conversation hash (helps routing on older models, harmless on new);
  effort changes via `configuration_update` where supported.

### Streaming events used
`response.created`, `response.output_item.added`, `response.output_text.delta`,
`response.reasoning_summary_text.delta`, `response.function_call_arguments.delta`,
`response.output_item.done`, `response.completed` / `response.incomplete` / `response.failed`, `error`.

---

## 2. DeepSeek

| | |
|---|---|
| Protocol used by Ycode | **Chat Completions** `POST /chat/completions` (OpenAI-format, DeepSeek-specific fields) |
| Base URL | `https://api.deepseek.com` |
| List models | `GET /models` — returns `context_window`, `max_output_tokens`, `input_modalities`, `effort.supported_levels`, `effort.default_level` |

DeepSeek also offers a Responses API (`/responses`, stateless, no `previous_response_id`, no `include`,
no `prompt_cache_key`, reasoning `summary` ignored, reasoning-item replay merged into the assistant
message) and an Anthropic-format endpoint (`/anthropic`). **Ycode uses Chat Completions** because its
thinking + tool-call replay rules (`reasoning_content` must be returned on every later request when
`tools` are present, else 400) and its cache accounting (`prompt_cache_hit_tokens` +
`prompt_cache_miss_tokens` = a reliable denominator) are explicitly documented there. It is a separate
adapter, not the OpenAI adapter with a different base URL.

| Model ID | Served by | Context | Max out | Tools | Thinking | Vision | Effort levels | Ycode use |
|---|---|---|---|---|---|---|---|---|
| `deepseek-v4-pro` | DeepSeek-V4-Pro-0813 | 1M | 384K | yes | on by default | no | low, high, max (+ none = off) | **Coding default** |
| `deepseek-flash` | DeepSeek-V4.1-Flash-1017 | 1M | 384K | yes | on by default | yes | low, high, max (+ none = off) | **Office default** |

Legacy names `deepseek-v4-flash`, `deepseek-v4-flash-vision-exp` are still accepted but retired (routed to
Flash). `deepseek-chat` / `deepseek-reasoner` are no longer mentioned in the docs → not offered.

### Thinking / effort
* `thinking: {"type":"enabled"|"disabled"}` (takes precedence) and `reasoning_effort: none|low|high|max`
  (default `high`; `minimal→low`, `medium/xhigh→high` are aliases — Ycode never sends aliases).
* In thinking mode `temperature`/penalties are ignored; `tool_choice` `required`/named → 400. Ycode uses `auto`.
* With `tools`, every previous assistant turn's `reasoning_content` must be replayed. Ycode stores it.
* No documented mechanism for "effort change without cache impact"; changing `reasoning_effort` is just
  a parameter — whether it affects the KV prefix is **not documented** (see cache tests).

### Context caching
* Automatic for all users; best-effort; prefix units created at end of each user input / model output,
  for shared prefixes, and at fixed token intervals. Cleared after hours–days unused.
* Usage: `prompt_cache_hit_tokens`, `prompt_cache_miss_tokens` (also `prompt_tokens_details.cached_tokens`).
* Price (per 1M, off-peak/peak): Flash hit $0.003/$0.006, miss $0.15/$0.30, out $0.6/$1.2;
  V4-Pro hit $0.022/$0.044, miss $0.66/$1.32, out $1.98/$3.96.
* **Ycode policy:** no cache parameters exist → send none; keep prefix byte-stable; hit rate =
  hit / (hit + miss).

### Streaming
`choices[].delta.{content, reasoning_content, tool_calls[{index,id,function{name,arguments}}]}`,
`finish_reason ∈ stop|length|content_filter|tool_calls|insufficient_system_resource|aborted`,
`stream_options.include_usage: true` → final chunk carries usage, then `data: [DONE]`.

---

## 3. Anthropic (Claude)

| | |
|---|---|
| Protocol | **Messages API** `POST /v1/messages`, raw HTTP (no official Rust SDK) |
| Headers | `x-api-key`, `anthropic-version: 2026-10-21`, `anthropic-beta` (comma list) when needed |
| List models | `GET /v1/models` → `id, display_name, created_at, max_input_tokens, max_tokens, capabilities, line` |

| Model ID | Thinking | Default effort | Effort values | Context | Max out | Ycode use |
|---|---|---|---|---|---|---|
| `claude-fable-5-1` | adaptive, always on | high | low, medium, high, xhigh, max | 1M | 128K | demanding coding |
| `claude-opus-5-5` | adaptive, always on (`disabled` → 400) | **medium** | low, medium, high, xhigh, max | 1M | 128K | **Coding default** |
| `claude-sonnet-5-5` | adaptive (`disabled` → 400) | high | low, medium, high, xhigh, max | 1M | 128K | **Office default** |
| `claude-haiku-5-5` | adaptive | medium | low, medium, high, xhigh, max | 1M | 128K | light/fast |

Legacy but available: Fable 5, Opus 5, Opus 4.8/4.7/4.6/4.5, Sonnet 5, Sonnet 4.6, Haiku 4.5. All
current models: text+image input, tool use. Retirement not before Sep–Oct 2027.

### Effort & thinking
* `output_config: {"effort": "..."}` (GA). Ycode sends `thinking: {"type":"adaptive","display":"summarized"}`
  so a readable reasoning summary can stream; `budget_tokens` is never sent (400 on current models).
* **Mid-conversation effort change without invalidating the messages cache:** beta
  `mid-conversation-output-config-2026-07-01` — append `{"role":"system","content":[],"output_config":{"effort":"..."}}`
  to `messages` (Fable 5.1, Opus 5.5, Opus 5, Sonnet 5.5, Haiku 5.5; thinking on). Ycode uses this for
  those models; otherwise it changes top-level effort and accepts the messages-cache miss.
* Forced `tool_choice` (`any`/`tool`) → 400 on Fable 5.1 / Opus 5.5 / Sonnet 5.5 → Ycode uses `auto`.
* Thinking blocks are passed back **unchanged** (preserved thinking: editing history invalidates them) →
  Ycode's history is append-only.
* `temperature/top_p/top_k` → 400 on API version 2026-10-21 → never sent.
* `stop_reason: "refusal"` handled explicitly. Server-side fallbacks (`fallbacks: "default"`, beta
  `server-side-fallback-2026-07-01`) are enabled by default for Fable 5.1 / Opus 5.5 / Opus 5 / Sonnet 5.5
  (Settings toggle). Echo rule after a mid-output fallback implemented (drop thinking/tool_use blocks
  before the last `fallback` block).
* Tools are sent with `eager_input_streaming: true`; tool input is validated after parsing.

### Prompt caching
* Top-level automatic `cache_control: {"type":"ephemeral"}` (moving breakpoint at last cacheable block)
  plus explicit breakpoints on content blocks; max 4 breakpoints; hash order `tools → system → messages`;
  20-block lookback.
* TTL: 5 min default (refreshed on use) or `"ttl":"1h"`.
* Minimum cacheable: 512 tokens (Fable 5.1, Opus 5.5, Sonnet 5.5, Haiku 5.5, …), 1,024–4,096 on older.
* Usage: `cache_creation_input_tokens`, `cache_read_input_tokens`, `input_tokens` (after last breakpoint),
  `cache_creation.{ephemeral_5m_input_tokens, ephemeral_1h_input_tokens}`.
  Total input = read + creation + input_tokens.
* Prices: writes 1.25× (5m) / 2× (1h); reads 0.1× (0.05× Opus 5.5 / Sonnet 5.5, 0.025× Fable 5.1).
* Invalidators: tool defs (all), `tool_choice`, images added/removed, thinking params and **effort**
  (messages level), speed.
* **Ycode policy:** explicit breakpoint on the last system block (tools+system), top-level automatic
  breakpoint for the growing conversation (2 of 4 slots). Dynamic per-turn context is placed in the
  latest user message, never in `system`.

---

## 4. Capability matrix (as configured in Ycode)

| Capability | OpenAI adapter | DeepSeek adapter | Anthropic adapter |
|---|---|---|---|
| Streaming | SSE typed events | SSE chunks | SSE typed events |
| Tool calling | ✔ function | ✔ function | ✔ tool_use |
| Image input | model supports; **Ycode V1: not wired** | Flash only; not wired | supports; not wired |
| PDF/doc input | not wired in V1 | not wired | not wired |
| Effort control | `reasoning.effort` | `reasoning_effort` + `thinking` | `output_config.effort` |
| Effort change w/o cache break | `configuration_update` item (GPT-6) | undocumented | per-message system effort (beta) |
| Cache params sent | `prompt_cache_key` | none | `cache_control` (auto + system breakpoint) |
| Cache read field | `input_tokens_details.cached_tokens` | `prompt_cache_hit_tokens` | `cache_read_input_tokens` |
| Cache write field | `input_tokens_details.cache_write_tokens` | n/a | `cache_creation_input_tokens` |
| Hit-rate denominator | `input_tokens` | hit + miss | read + creation + input |
| Cancel | drop HTTP stream | drop HTTP stream | drop HTTP stream |

## 5. Distinctions required by the spec
* **Latest released:** GPT-6.1 Sol/Terra (OpenAI), DeepSeek-V4.1-Flash-1017 (`deepseek-flash`), Claude Fable 5.1 / Opus 5.5 / Sonnet 5.5 / Haiku 5.5.
* **Latest stable API models:** all IDs in the tables above are GA per the vendor pages; no preview/experimental IDs are defaults.
* **Models your key can access:** only knowable at runtime → Settings › Provider › *Refresh models* lists exactly what `/models` returns for your key; unlisted defaults are shown as "not returned by your key".

## 6. Not verified / open items
* No live API calls were made during development (no keys available). Request shapes follow the docs verbatim; first live run must follow `docs/TESTING.md` § Live checks.
* OpenAI streaming event schemas for `response.completed.usage` were taken from the prompt-caching and reasoning guides; the full streaming reference page was too large to read in one pass.
* DeepSeek: whether changing `reasoning_effort` changes the cached prefix is undocumented.
* OpenAI `configuration_update` support list is described as "supported GPT-6 models"; Ycode flags all four GPT-6 flagship IDs — verify on first live run.

## Sources
* https://developers.openai.com/api/docs/models
* https://developers.openai.com/api/reference/resources/models/methods/list
* https://developers.openai.com/api/docs/guides/prompt-caching
* https://developers.openai.com/api/docs/guides/reasoning
* https://developers.openai.com/api/docs/guides/function-calling?api-mode=responses
* https://api-docs.deepseek.com/ , /api/list-models/ , /guides/thinking_mode/ , /guides/kv_cache/ , /quick_start/pricing , /api/create-chat-completion/ , /guides/responses_api/
* https://platform.claude.com/docs/en/models/overview
* https://platform.claude.com/docs/en/build-with-claude/effort
* https://platform.claude.com/docs/en/build-with-claude/prompt-caching
* Anthropic `claude-api` reference skill (model table cached 2026-10-06, cross-checked against the live overview page above)
