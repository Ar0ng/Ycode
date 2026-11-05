# Ycode

一款轻量化的 Windows AI Agent，由 GPT、DeepSeek 和 Claude 提供支持。

A lightweight Windows desktop AI agent. Chat with **OpenAI GPT**, **DeepSeek** and
**Anthropic Claude** through their official APIs, switch between an **Office** mode
for writing and analysis and a **Coding** mode that reads, edits and runs code in a
folder you choose, add **Agent Skills**, set the model's real **reasoning effort**, and
update from **GitHub Releases**.

Built with Tauri 2 (Rust) + React + TypeScript. Local-only: no account, no cloud
sync, no telemetry.

> **Taking over development?** Read [HANDOFF.md](HANDOFF.md) first. It covers the
> architecture, invariants, current status, next tasks, known issues and conventions.

**Status (2026-11-05, v0.1.3):**
- CI is green.
- 77 + 1 Rust tests and 11 frontend tests pass.
- A signed NSIS installer builds and installs.
- Provider integrations are verified against documentation and mocks only, not yet against live API keys.
- No GitHub Release has been published yet.

## Features

- **Three native provider adapters**, one internal interface:
  OpenAI Responses API · DeepSeek Chat Completions (thinking mode) · Claude Messages API.
  Streaming, tool calling, cancellation, retries, typed errors, token & cache usage.
- **Model management** — built-in capability table (see [MODEL_RESEARCH.md](MODEL_RESEARCH.md)),
  live *Refresh models* per provider, hand-typed model IDs.
- **Reasoning effort dropdown** — shows only the levels the selected model accepts
  (with its default and the exact value sent), remembered per model; uses in-band updates
  (OpenAI `configuration_update`, Claude per-message effort) to keep the prompt cache.
- **Office / Coding modes** — Office: no terminal, writing-oriented instructions.
  Coding: list/search/read/edit/write files and run PowerShell commands, with diff
  previews and per-action approval (Pending → Running → Success/Failed/Cancelled).
- **Safety** — workspace path confinement (no `..`/symlink escapes, no writes outside
  the folder, reads outside need approval), command deny-list + risk warnings, Stop
  kills running commands, tool output is treated as data.
- **Skills** — agentskills.io format, validated on install, enable/disable/delete,
  progressive disclosure (`load_skill`, `read_skill_file`), `$skill-name` mentions.
- **Prompt-cache aware context** — frozen system prompt, stable tool schema,
  append-only history, provider-specific cache parameters, per-request and per-chat
  cache hit rate (N/A when the provider gives no reliable denominator).
- **Image generation** — GPT Image (`gpt-image-2.6-flare` by default) as `generate_image` /
  `edit_image` tools usable from any chat model; images shown inline, saved locally.
- **Sidebar sections** — create, rename, collapse and delete sections; start a chat in a
  section or move chats between sections (stored locally, schema v2).
- **Model list hygiene** — "Hide older models" (GPT below 5.5 / older minis, Claude 4.x and
  older, retired DeepSeek names) plus per-model hide/show.
- **Bilingual UI** (简体中文 / English), light/dark/system theme, collapsible sidebar.
- **Signed auto-update** via GitHub Releases; never installs without confirmation.

## Docs

| | |
|---|---|
| [HANDOFF.md](HANDOFF.md) | **start here when continuing development** |
| [docs/BUILD.md](docs/BUILD.md) | toolchain, dev commands, release build, project layout |
| [docs/API_SETUP.md](docs/API_SETUP.md) | keys, base URLs, model refresh, effort levels, errors |
| [docs/SKILLS.md](docs/SKILLS.md) | skill format, install, progressive loading, safety |
| [docs/RELEASING.md](docs/RELEASING.md) | signing key, GitHub secrets, tagging, updater |
| [docs/TESTING.md](docs/TESTING.md) | automated tests, manual checks, results |
| [docs/KNOWN_LIMITATIONS.md](docs/KNOWN_LIMITATIONS.md) | what V1 does not do yet |
| [MODEL_RESEARCH.md](MODEL_RESEARCH.md) | model IDs, capabilities, caching rules (2026-11-05) |
| [cache-results/](cache-results/README.md) | live cache probe scenarios and results |

## Quick start (users)

1. Install `Ycode_<version>_x64-setup.exe` (per-user, no admin rights).
2. Settings → Models & API → paste a key for at least one provider → *Test & refresh models*.
3. On a new chat, pick **Office** or **Coding** in the top bar. Pick the model and the reasoning level under the input box.
4. For Coding mode, choose a workspace folder with the folder button at the top right.

## Quick start (developers)

**Prerequisites:**
- Windows 10/11 x64
- Node.js 24 (npm 11)
- Rust stable with the `x86_64-pc-windows-msvc` target
- Visual Studio 2022 Build Tools with "Desktop development with C++" and a Windows SDK
- WebView2 runtime

```bash
npm ci
```
```bash
npm run tauri dev
```
`npm run tauri dev` builds the Rust backend and opens the app with hot reload.

| Task | Command |
|---|---|
| UI only, in a browser, fake backend | `npm run dev` → http://localhost:1420 |
| Type-check + frontend tests | `npm run typecheck` · `npm test` |
| Rust tests + lints | `cd src-tauri` · `cargo test` · `cargo clippy --all-targets -- -D warnings` |
| Signed installer | `scripts\build-release.ps1` (needs the signing key, see [docs/RELEASING.md](docs/RELEASING.md)) |
| Unsigned installer | `npm run tauri build -- --config '{"bundle":{"createUpdaterArtifacts":false}}'` |

Output: `src-tauri/target/release/bundle/nsis/Ycode_<version>_x64-setup.exe`.

### Project map

```
src/                  React UI: components/, lib/ (store, IPC api, i18n, types), dev/mock.ts
src-tauri/src/
  agent/              agent loop, effort planning, prompts, end-to-end tests (mock HTTP server)
  providers/          openai.rs · deepseek.rs · anthropic.rs · registry.rs · sse.rs · mod.rs
  tools/              file/search/edit/shell tools, sandbox.rs (path confinement)
  images.rs skills.rs storage.rs secrets.rs state.rs commands.rs updater.rs model.rs error.rs
src-tauri/tests/      cache_probe.rs (live, ignored) · signature.rs (installer signature)
scripts/              build-release.ps1 · install-test.ps1
docs/                 BUILD, API_SETUP, SKILLS, RELEASING, TESTING, KNOWN_LIMITATIONS
.github/workflows/    ci.yml (every push) · release.yml (tags v*)
```

## Data locations

| What | Where |
|---|---|
| Chats, settings, skills, logs | `%APPDATA%\com.ycode.desktop\` |
| API keys | Windows Credential Manager, `Ycode` / `openai` · `deepseek` · `anthropic` |

Never commit API keys, `.env` files or the updater signing key (`%USERPROFILE%\.ycode-signing\`).
`.gitignore` blocks them; CI gets the signing key from GitHub Secrets.

## License

Not yet chosen by the repository owner.
