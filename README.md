# Ycode

一款轻量化的 Windows AI Agent，由 GPT、DeepSeek 和 Claude 提供支持。

A lightweight Windows desktop AI agent. Chat with **OpenAI GPT**, **DeepSeek** and
**Anthropic Claude** through their official APIs, switch between an **Office** mode
for writing and analysis and a **Coding** mode that reads, edits and runs code in a
folder you choose, add **Agent Skills**, set the model's real **reasoning effort**, and
update from **GitHub Releases**.

Built with Tauri 2 (Rust) + React + TypeScript. Local-only: no account, no cloud
sync, no telemetry.

## Features

- **Three native provider adapters**, one internal interface:
  OpenAI Responses API · DeepSeek Chat Completions (thinking mode) · Claude Messages API.
  Streaming, tool calling, cancellation, retries, typed errors, token & cache usage.
- **Model management** — built-in capability table (see [MODEL_RESEARCH.md](MODEL_RESEARCH.md)),
  live *Refresh models* per provider, hand-typed model IDs.
- **Reasoning effort slider** — shows only the levels the selected model accepts and
  sends exactly that value; remembered per model; uses in-band updates
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
| [docs/BUILD.md](docs/BUILD.md) | toolchain, dev commands, release build, project layout |
| [docs/API_SETUP.md](docs/API_SETUP.md) | keys, base URLs, model refresh, effort levels, errors |
| [docs/SKILLS.md](docs/SKILLS.md) | skill format, install, progressive loading, safety |
| [docs/RELEASING.md](docs/RELEASING.md) | signing key, GitHub secrets, tagging, updater |
| [docs/TESTING.md](docs/TESTING.md) | automated tests, manual checks, results |
| [docs/KNOWN_LIMITATIONS.md](docs/KNOWN_LIMITATIONS.md) | what V1 does not do yet |
| [MODEL_RESEARCH.md](MODEL_RESEARCH.md) | model IDs, capabilities, caching rules (2026-11-05) |
| [cache-results/](cache-results/README.md) | live cache probe scenarios and results |

## Quick start

1. Install `Ycode_<version>_x64-setup.exe` (per-user, no admin rights).
2. Settings → Models & API → paste a key for at least one provider → *Test & refresh models*.
3. Pick a model and mode under the input box and start typing.
4. For Coding mode, choose a workspace folder with the folder button.

## Data locations

| What | Where |
|---|---|
| Chats, settings, skills, logs | `%APPDATA%\com.ycode.desktop\` |
| API keys | Windows Credential Manager, `Ycode` / `openai` · `deepseek` · `anthropic` |

## License

Not yet chosen by the repository owner.
