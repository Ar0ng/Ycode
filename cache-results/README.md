# Cache test results

Live prompt-cache results produced by `src-tauri/tests/cache_probe.rs`
(`cargo test --test cache_probe -- --ignored --nocapture`, keys via
`YCODE_*_KEY` environment variables). Each run writes one JSON file per provider.

**Status (2026-11-05): no live results yet.** No API keys were available during
development, so nothing here has been measured against the real services. The
cache-relevant request properties are covered by offline tests instead (see
`docs/TESTING.md` › Prompt caching).

## Scenarios

| Step | What changes | Expected |
|---|---|---|
| 1a / 1b | identical request twice | 1b hits the system + tools prefix |
| 2 | append a turn | hit up to the previous turn |
| 3 | new dynamic user text at the end | hit up to the previous turn, tail uncached |
| 4 | reasoning effort changed | OpenAI GPT-6 / Claude 5.x: hit (in-band update); DeepSeek: undocumented |
| 5 | skill index added to system | miss after the system block |
| 6 | Office → Coding (tools + system change) | miss |
| 7 | repeat after 6 | hit again |

Switching provider always starts a fresh cache (caches are per vendor and per model).

## How to read a result

`hit_rate = cached_input_tokens / eligible_input_tokens`, where the denominator
is the provider's own total-input figure (OpenAI `input_tokens`; DeepSeek
`prompt_cache_hit_tokens + prompt_cache_miss_tokens`; Claude
`cache_read + cache_creation + input_tokens`). It is `null` (shown as N/A in the
app) when the prompt is below the provider's minimum cacheable length or the
provider didn't report the split.

Typical causes of an unexpected miss: more than 5 minutes (Claude default TTL) or
30 minutes (OpenAI GPT-5.6+ default) since the previous request, routing to a
different cache shard (OpenAI pre-5.6), the prompt being under the minimum
length, or DeepSeek's best-effort cache having been evicted.
