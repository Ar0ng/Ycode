# Ycode — Developer handoff

Last updated: **2026-11-06** (v0.2.1).
Read this file first, then `README.md`, then the doc that matches your task (table at the end).

> **Note for the agent releasing V0.2.2 (2026-11-06):** `git pull` `main` before you start —
> it includes `3946eee` (installer .exe now uses the Ycode icon via
> `bundle.windows.nsis.installerIcon`); keep that line in `tauri.conf.json`. Release
> checklist: bump the 3 version fields → `cd src-tauri && cargo update -p ycode` →
> push → tag `v0.2.2`. Leave `main` pushed and clean when done.

## 1. What Ycode is

Ycode is a Windows desktop AI agent built with Tauri 2: a Rust backend and a React + TypeScript UI.
It chats with **OpenAI (Responses API)**, **DeepSeek (Chat Completions)** and **Claude (Messages API)**,
and it has two modes:

- **Office**: writing and analysis, no terminal.
- **Coding**: reads, edits and runs code in a folder the user chooses.

Other features: Agent Skills, GPT Image generation, sidebar sections, and signed auto-update
from GitHub Releases. All data stays on the user's machine. API keys live in Windows
Credential Manager.

| | |
|---|---|
| Repository | https://github.com/Ar0ng/Ycode (branch `main`) |
| Local path (original machine) | `I:\ClaudeData\Ycode` |
| Current version | 0.2.1 (shown in the app as "V0.2.1") (`package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` — keep all three equal) |
| App identifier / data dir | `com.ycode.desktop` → `%APPDATA%\com.ycode.desktop\` (`ycode.db`, `images\`, `skills\`, `logs\`) |
| DB schema version | 2 (`PRAGMA user_version`) |
| Code size | Rust ≈6.8k lines (incl. tests) · TS/TSX ≈3.4k lines |

## 2. Status at handoff

| Check | Result (2026-11-05) |
|---|---|
| `npm run typecheck` | pass |
| `npm test` (vitest) | 11 passed |
| `npx vite build` | pass (JS bundle ≈475 KB / 146 KB gzip) |
| `cargo test --locked` | 77 passed + 1 (installer signature), 2 ignored by design |
| `cargo clippy --all-targets -- -D warnings` | clean |
| GitHub Actions CI on `main` | green for every pushed commit |
| Signed release build | `Ycode_0.1.3_x64-setup.exe` (3.95 MB) built and installed locally |
| Installed app | 0.1.3 runs; v1 → v2 DB migration ran on real user data without loss |

The working tree is clean and `main` equals `origin/main`. No GitHub Release has been
published yet (see §7).

## 3. Architecture

```
React UI (src/)                         Rust backend (src-tauri/src/)
───────────────                         ─────────────────────────────
lib/store.ts  (zustand)  ──invoke──▶    commands.rs     IPC surface (all #[tauri::command])
lib/api.ts    (typed IPC)  ◀─Channel─   agent/mod.rs    agent loop, effort planning, approvals
components/*                            providers/      openai.rs · deepseek.rs · anthropic.rs
                                                        mod.rs (run_turn, SSE driver, retries)
                                                        registry.rs (model capability table)
                                                        sse.rs (SSE parser)
                                        tools/          mod.rs (tool specs, prepare/execute)
                                                        sandbox.rs (path confinement)
                                                        shell.rs (PowerShell, deny-list, kill tree)
                                        images.rs       GPT Image (Images API)
                                        skills.rs       Agent Skills install/validate/load
                                        storage.rs      SQLite (WAL), migrations, settings JSON
                                        secrets.rs      Windows Credential Manager (keyring v3)
                                        state.rs        AppState (db, http, settings, runs, approvals)
                                        updater.rs      tauri-plugin-updater, GitHub latest.json
                                        model.rs        provider-neutral Message/ToolCall/Usage
                                        error.rs        AppError / ProviderError (+ classification)
                                        logging.rs      file log with key redaction
```

### Request flow (one user message)
1. The UI calls `send_message(params, Channel<AgentEvent>)`. The backend registers a cancellable run and spawns `agent::run`.
2. `run_inner` does the following:
   - loads or creates the conversation;
   - plans the reasoning effort (`plan_effort`, which may append an in-band `Control` message);
   - saves the user message;
   - builds the system blocks (`agent/prompts.rs`) and the tool specs.
3. Loop, up to `max_steps`:
   - `providers::run_turn` builds the vendor request (`build`), streams SSE through the vendor `Decoder`, and returns `TurnOutput`.
   - The assistant message is persisted. Vendor-native replay data goes in `raw`: OpenAI output items, Claude content blocks, or DeepSeek `reasoning_content`.
   - Each tool call goes through `tools::prepare`. If it needs approval, an `ApprovalPrompt` event is sent and the call waits for `respond_approval`; then `tools::execute_full` runs it.
   - All results are persisted as one `Role::Tool` message.
4. Events stream to the UI: `message`, `delta`, `tool_status`, `usage`, `notice`, `error`, `title`, `finished`.

### Invariants — do not break
- **History is append-only.** Never rewrite earlier messages. Prompt caching and Claude's "preserved thinking" both depend on this. The only exception is `trim_context`, which runs only near the context limit.
- **Request prefixes are stable.** The system blocks contain no timestamps or random values. The tool list order is fixed (`tools::specs_with`). Request-level effort stays at the conversation's base value when the provider supports in-band changes. Tests assert that request N+1 is a byte-extension of request N.
- **Keys never reach the UI.** The UI only learns `has_key`. Tests never read real keys: `state.rs` uses `cfg!(test)`, and keys are injected through `key_override`.
- **Effort values come from the capability table or the vendor's list-models data.** Never guess them from a model name. Unknown models have effort disabled.
- **Zustand selectors must not build new arrays or objects.** Select the stable value and derive it with `useMemo`; otherwise the app re-renders forever and the window goes blank. A vitest test enforces this.
- **Agent tool safety:**
  - writes are confined to the workspace, and `.git` is never written;
  - reads outside the workspace need approval;
  - every terminal command needs approval;
  - system-destructive commands are blocked outright (`shell::blocked`).

### Vendor specifics (details and sources in `MODEL_RESEARCH.md`)

| | OpenAI | DeepSeek | Anthropic |
|---|---|---|---|
| Endpoint | `POST /responses` (`store:false`) | `POST /chat/completions` | `POST /v1/messages` (`anthropic-version: 2026-10-21`) |
| Effort | `reasoning.effort` (+ `summary:auto`) | `thinking` + `reasoning_effort` (off/low/high/max) | `output_config.effort` + adaptive thinking |
| Mid-chat effort change | `configuration_update` input item | parameter change | per-message system msg (beta header) |
| Cache params | `prompt_cache_key` | none (automatic) | explicit system breakpoint + top-level `cache_control` |
| Hit-rate denominator | `input_tokens` | hit + miss | read + creation + input |

## 4. Implemented features

- Three provider adapters with:
  - streaming, tool calling, retries for retryable errors (only while nothing has streamed yet), cancel/stop that keeps partial text;
  - typed, localised errors.
- Model registry (`providers/registry.rs`):
  - built-in capability table, plus *Refresh models* through each vendor's `/models`, plus model IDs typed by hand;
  - "Hide older models" and per-model hide in Settings (`src/lib/models.ts`).
- Reasoning effort dropdown (`EffortPicker.tsx`): only real levels, the model's default, and the exact value sent; remembered per model.
- Office/Coding switch in the top bar, shown on new chats only; opening a chat restores its mode. Workspace folder picker with recent folders.
- Coding tools: `read_file`, `list_dir`, `search_files` (respects .gitignore), `write_file`/`edit_file` (diff and approval), `run_command`.
- Office tools: read, list, search, write — no terminal.
- GPT Image: `generate_image` / `edit_image` tools through the Images API (default `gpt-image-2.6-flare`), usable from any chat model when an OpenAI key exists. Images are shown inline, with full-size view and Save as.
- Agent Skills (agentskills.io):
  - install from a folder, validate, enable/disable/delete;
  - progressive loading through `load_skill` / `read_skill_file`;
  - `$skill-name` mentions in a message.
- Sidebar sections: create, rename, collapse, delete, new chat in a section, move chats.
- Copy button on every message; Markdown with copy buttons on code blocks.
- Usage line per request and per chat: input, output, cached tokens and hit rate, or N/A when there is no reliable denominator.
- zh-CN / en-US UI; light, dark or system theme; collapsible sidebar; error boundaries (app-wide and main area).
- Text attachments (.txt/.md/.log/.csv). Images, PDF and Office files are validated but blocked with "not supported yet".
- Signed NSIS per-user installer; updater checks `Ar0ng/Ycode` releases; CI and release workflows.

## 5. Not done yet / next tasks (suggested order)

1. **Live API verification.** No real key has been used in development. Follow `docs/TESTING.md` › Live checks for all three providers, then run `cargo test --test cache_probe -- --ignored --nocapture` with `YCODE_*_KEY` set and commit the results to `cache-results/`. The most likely places to need fixes are listed in `docs/KNOWN_LIMITATIONS.md`.
2. **First GitHub Release.**
   - Add the secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (the owner has the files).
   - Tag `v0.1.3` and push the tag.
   - Then verify an in-app update end to end with a later `v0.1.4`.
3. **Image, PDF and Office attachments** to vision-capable models. `Attachment`/`AttachmentKind` and `ModelCaps.vision` exist, and `Message::user_content_text` is the place to extend.
4. Parallel tool execution (tools currently run sequentially), and summarisation-based context compaction (currently only old tool output is trimmed).
5. Persist "Allow for this chat" approvals (currently in memory only).
6. Code-signing certificate (SmartScreen warns on install), and a Windows 10 test pass.

## 6. Known issues and gotchas

- **The Claude desktop app sandbox redirects `%LOCALAPPDATA%`** for processes it spawns. This only matters if you are an agent running inside the Claude desktop app. It breaks the NSIS step of `tauri build` (os error 17) and would redirect installs. Workaround: start `scripts\build-release.ps1` and `scripts\install-test.ps1` through `explorer.exe`, as described in `docs/BUILD.md`. CI and normal terminals are not affected.
- **Vite's file watcher sometimes misses rapid consecutive edits** in that same environment, so the dev page goes blank or shows a stale error. Restart `npm run dev` before debugging.
- **`src/dev/mock.ts`** is a fake backend for `npm run dev` in a plain browser. It returns `structuredClone`d data, like real IPC. Keep it in sync when you add fields to `Settings` or `Bootstrap` (`tsc` will tell you).
- **Image edits** send `size`, `quality` and `n` as multipart fields, but the docs' multipart example lists only `image[]`, `prompt` and `model`. Verify this live.
- **OpenAI models outside the built-in table** have effort disabled until they are added to `registry::builtin()`, because OpenAI `/models` returns no capability data.
- **Tests must not touch real keys or real data.** The Credential Manager round-trip test is `#[ignore]` and uses its own entry.

## 7. Commands

Prerequisites: Windows 10/11 x64, Node 24 + npm 11, Rust stable (MSVC target), VS 2022 Build
Tools (C++ workload + Windows SDK), WebView2. On the original machine Rust lives on D:, so set:
```powershell
$env:RUSTUP_HOME='D:\DevTools\rust\rustup'; $env:CARGO_HOME='D:\DevTools\rust\cargo'; $env:Path="D:\DevTools\rust\cargo\bin;$env:Path"
```

| Task | Command |
|---|---|
| Install JS deps | `npm ci` |
| Run the app (hot reload) | `npm run tauri dev` |
| UI only in a browser (mock backend) | `npm run dev` → http://localhost:1420 |
| Type-check / frontend tests | `npm run typecheck` · `npm test` |
| Rust tests | `cd src-tauri && cargo test` |
| Lints (CI uses this) | `cd src-tauri && cargo clippy --all-targets -- -D warnings` |
| Signed release build | `powershell -ExecutionPolicy Bypass -File scripts\build-release.ps1` (needs `%USERPROFILE%\.ycode-signing\`), or set `TAURI_SIGNING_PRIVATE_KEY*` and run `npm run tauri build` |
| Unsigned local installer | `npm run tauri build -- --config '{"bundle":{"createUpdaterArtifacts":false}}'` |
| Installer smoke test | `powershell -ExecutionPolicy Bypass -File scripts\install-test.ps1 -Setup <setup.exe> -Out result.json` |
| Release | bump the 3 version fields → `cd src-tauri && cargo update -p ycode` (refreshes Cargo.lock; CI uses `--locked`) → commit → `git tag vX.Y.Z` → `git push origin main --tags` |

Output: `src-tauri/target/release/bundle/nsis/Ycode_<ver>_x64-setup.exe` (+ `.sig`).

## 8. Secrets and what never goes into git

| Secret | Location | In repo? |
|---|---|---|
| Provider API keys | Windows Credential Manager, `Ycode` / `openai`·`deepseek`·`anthropic` | never |
| Updater signing private key + password | `%USERPROFILE%\.ycode-signing\ycode.key`, `password.txt` (owner's machine; back them up) | never; GitHub Secrets for CI |
| Updater public key | `src-tauri/tauri.conf.json` › `plugins.updater.pubkey` | yes (public by design) |

`.gitignore` covers `.env*`, `*.key`, `*.pem`, `password.txt`, `.ycode-signing/`, `*.db*`,
`*.sqlite*`, installers and `.sig` files, `node_modules`, `dist`, `src-tauri/target`, and logs.
A history scan on 2026-11-05 found no keys or signing material.

## 9. Conventions

- Commit messages: imperative subject, a short body, and the trailer `Co-Authored-By:` when an AI wrote the commit. Bump the version for user-visible releases.
- Every new feature needs tests:
  - Rust: unit tests next to the code; agent end-to-end tests in `src-tauri/src/agent/tests.rs` use the local mock HTTP server.
  - Frontend: `src/lib/frontend.test.ts`.
- UI strings: add the key to **both** `en` and `zh` in `src/lib/i18n.ts`; a test checks parity.
- New settings fields:
  - Rust: add them to `Settings` + `Default` in `storage.rs`; if the UI may change them, add them to `PATCHABLE` in `commands.rs`.
  - TS: add them to `Settings` in `src/lib/types.ts` and to the mock.
- DB changes: append to `MIGRATIONS` in `storage.rs` and bump `SCHEMA_VERSION`; never edit an existing migration. Add a test that upgrades the previous version.
- New IPC commands: add them to `commands.rs`, `lib.rs` `generate_handler!`, `src/lib/api.ts`, and capability permissions if they use a plugin.

## 10. Doc map

| File | Use it for |
|---|---|
| `README.md` | overview, quick start |
| `MODEL_RESEARCH.md` | model IDs, effort/caching/protocol facts with sources (2026-11-05) |
| `docs/BUILD.md` | toolchain, build, installer details, project layout |
| `docs/API_SETUP.md` | keys, base URLs, effort levels, image generation, error meanings |
| `docs/SKILLS.md` | skill format, loading, safety |
| `docs/RELEASING.md` | signing key, GitHub secrets, release and update flow |
| `docs/TESTING.md` | test coverage map, Windows package results, live-check checklist |
| `docs/KNOWN_LIMITATIONS.md` | V1 limits and unverified assumptions |
| `cache-results/README.md` | prompt-cache probe scenarios (no live results yet) |
