# Testing

Results below are from 2026-11-05 on the development machine (Windows 11 Pro 26200,
x64). Nothing in this file was produced with a real API key — see *Live checks*.

## Automated tests

```bash
npm test                          # vitest, frontend logic
```
```bash
npm run typecheck                 # tsc --noEmit
```
```bash
cd src-tauri && cargo test        # Rust unit + end-to-end agent tests
```
```bash
cd src-tauri && cargo test -- --ignored credential_manager_roundtrip   # touches the real Credential Manager
```

The Rust suite starts a **local mock HTTP server** that replays scripted SSE streams,
so the whole agent path (request building → streaming → tool approval → tool
execution → follow-up request → persistence) runs without spending API credit.

### Coverage map

| Area | Spec item | Test(s) |
|---|---|---|
| API | OpenAI / DeepSeek / Claude request shape | `providers::{openai,deepseek,anthropic}::tests::builds_*` |
| API | streaming decode (text, reasoning, tool calls, usage) for all 3 | `*::tests::decodes_stream*` |
| API | model list refresh parsing (OpenAI ids-only, DeepSeek effort levels, Claude capabilities) | `model_list_filters_non_chat`, `parses_remote_model_metadata`, `parses_models_capabilities` |
| API | effort parameter actually sent / unsupported never sent | `none_effort_disables_thinking`, `plan_effort_param_models_change_request_level`, `clamp_rejects_unsupported_levels`, frontend `effort` tests |
| API | typical errors (401/402/404/429/5xx, stream `error` events) | `error::tests::*`, `failed_event_is_error`, `invalid_key_is_reported_and_retry_works` |
| API | cancel / stop keeps partial output | `stop_keeps_partial_output`, `cancel_wakes_waiters` |
| Agent | read / list / search (respects .gitignore) | `tools::tests::read_list_search` |
| Agent | edit with diff, CRLF files, ambiguous/missing match | `edit_requires_approval_and_produces_diff` |
| Agent | approval: deny is respected; allow-for-chat applies | `write_needs_approval_and_deny_is_respected`, `approve_session_applies_write` |
| Agent | path traversal / outside-workspace writes / `.git` | `sandbox::tests::*`, `writes_outside_workspace_denied_reads_need_approval` |
| Agent | dangerous commands blocked, risky flagged, timeout, cancel kills tree | `shell::tests::*` |
| Agent | multi-step tool loop | `tool_loop_is_append_only` |
| Agent | Office mode has no terminal | `office_mode_has_no_terminal`, `simple_chat_persists_and_titles` |
| Skills | install / enable / disable / delete / load / resources / `$mention` | `install_enable_disable_load_delete` |
| Skills | format validation (name rules, description, compatibility) | `name_rules`, `rejects_invalid_skills`, `parses_frontmatter_variants`, `bundled_example_skill_is_valid` |
| Skills | resource path confinement | `read_skill_resource` assertions in `install_enable_disable_load_delete` |
| Storage | persistence, reopen, migration version, corrupt DB / settings recovery | `storage::tests::*` |
| Secrets | Credential Manager set/get/delete (real store) | `credential_manager_roundtrip` (ignored by default; **passed**) |
| Cache | append-only history (request N+1 extends request N byte-for-byte) | `tool_loop_is_append_only`, `effort_change_is_in_band_and_keeps_prefix`, `per_message_effort_keeps_message_prefix` |
| Cache | stable tool schema & system prompt | `tool_list_is_stable_and_mode_scoped`, `blocks_are_deterministic` |
| Cache | in-band effort change keeps request-level params | `openai_effort_change_uses_configuration_update` |
| Cache | usage parsing & hit-rate denominators per provider | `decodes_stream*`, frontend `usage` tests |
| Cache | context trim only touches old tool output | `trim_context_only_touches_old_tool_output` |
| Images | generate via tool → Images API request body, file saved, only id returned to model | `generate_image_tool_saves_and_reports_image` |
| Images | tools hidden without OpenAI key / when disabled | `image_tools_hidden_without_openai_key_or_when_disabled` |
| Images | id validation (no path escape), body shape, b64 decode, moderation → refusal | `images::tests::*` |
| Storage | v1 → v2 migration keeps chats; sections create/rename/collapse/delete/move | `migrates_v1_database_to_v2_keeping_chats`, `groups_create_move_delete` |
| UI | legacy-model rules per provider + manual hide | `frontend.test.ts › legacy models` |
| UI | no derived arrays/objects in store selectors (blank-window regression) | `frontend.test.ts › store selectors` |
| UI | i18n key parity zh-CN / en-US, interpolation, error kinds | `frontend.test.ts › i18n` |
| Updater | repository validation / endpoint | `updater::tests::repo_validation` |

### Results

| Suite | Result |
|---|---|
| `cargo test` | **69 passed**, 0 failed (lib) · `tests/signature.rs` **1 passed** · 2 ignored by design (live cache probe, Credential Manager) |
| `npm test` (vitest) | **8 passed** |
| `npm run typecheck` | passed |
| Credential Manager round-trip | 1 passed |
| `cargo clippy --all-targets` | clean with `-D warnings` |

## UI checks (manual, browser preview with mock backend)

Done in the dev preview (`npm run dev`, fake IPC) at 1180×760:
new-chat page (centered input, welcome, mode/model/effort controls), conversation
page (user/assistant distinction, reasoning toggle, tool cards with status and diff,
Markdown + code copy, usage line), model picker grouped by provider, effort slider
switching levels when the model changes (Claude 5 levels → DeepSeek off/low/high/max),
dark theme, Chinese UI, Settings tabs.

## Windows package

Signed release build via `scripts/build-release.ps1` (NSIS, per-user), then
`scripts/install-test.ps1` (silent install → launch → measure → restart → silent
uninstall). Both scripts must run outside any app-package sandbox (on this machine they
were started through Explorer, because the Claude desktop app virtualises `%LOCALAPPDATA%`,
which broke the NSIS tool download inside its own process tree).

| Check | Result |
|---|---|
| Release build time (incremental Rust, LTO) | 130 s |
| Installer `Ycode_0.1.0_x64-setup.exe` | **3.86 MB** |
| Updater signature `.sig` verifies against `tauri.conf.json` pubkey | passed (`tests/signature.rs`; tampered file rejected) |
| Silent per-user install (no admin prompt) | exit 0, 1.1 s |
| Installed size (`%LOCALAPPDATA%\Ycode`) | 11 MB |
| Start-menu shortcut / uninstaller present | yes / yes |
| Main window visible after launch | 0.17 s (window created; UI paints shortly after) |
| Console window | none (GUI subsystem) |
| UI reached the Rust backend (IPC + CSP OK) | yes (`ui bootstrap` in log; screenshot shows the new-chat page) |
| Processes (Ycode + WebView2) | 7 |
| Idle memory, 8 s after start, all 7 processes | **365–372 MB** working set (two runs) |
| Idle CPU over 10 s | 0.03–0.05 % |
| Database + log created in `%APPDATA%\com.ycode.desktop` | yes |
| Close and relaunch, data kept | yes |
| Silent uninstall | exit 0, exe removed, user data kept |

Not covered locally: Windows 10, a machine without WebView2 (installer bootstraps it),
the GitHub Actions release run (no remote yet), and an end-to-end in-app update
(needs a published release).

## Live checks (pending — need API keys)

Run once keys are available; record results here.

1. Settings → each provider → *Test & refresh models* → expect "Connected · N models".
2. Office mode, each provider: send "Hello" → streamed reply, usage line appears.
3. Switch effort mid-chat (GPT-6 / Claude 5.x) → reply still works; second request
   shows cached tokens.
4. Coding mode on a scratch folder: "Create hello.txt with 'hi' and then read it back"
   → approval card with diff → Allow → read_file → final answer.
5. Ask for `Remove-Item -Recurse C:\` → blocked by policy without prompting.
6. Press Stop mid-answer → partial text kept, "Stopped" label.
7. Wrong key → "API key was rejected" + Retry.
8. `cargo test --test cache_probe -- --ignored --nocapture` with `YCODE_*_KEY` set →
   JSON in `cache-results/`.
