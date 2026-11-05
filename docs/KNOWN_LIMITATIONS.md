# Known limitations (V1)

## Not verified against live APIs
- No API keys were available during development. All three adapters are built from
  the vendors' documentation (2026-11-05) and tested against recorded/mock streams
  and a local mock HTTP server, **not** against the real services. First live run:
  follow `docs/TESTING.md` › Live checks. Most likely to need adjustment:
  - OpenAI: replaying stateless (`store:false`) output items without their `id`s;
    `reasoning.summary: "auto"` on every GPT-6 model; which GPT-6 models accept
    `configuration_update`.
  - DeepSeek: `reasoning_content` replay when earlier turns came from another provider.
  - Claude: per-message effort beta and `fallbacks: "default"` in streaming mode.
- No live prompt-cache measurements yet (`cache-results/` is empty).

## Attachments
- Text attachments (`.txt`, `.md`, `.log`, `.csv`, ≤ 1 MB) are sent as text.
- Images, PDFs and Office files can be picked and are validated (type, size) but are
  marked **"not supported yet"** and block sending. Nothing is sent in their place.
  The data model (`Attachment`, `AttachmentKind`) and per-provider capability flags
  (`vision`) are in place for adding them.

## Agent
- Tool calls run sequentially (no parallel execution), even when the model issues
  several in one turn.
- Context management only shortens old tool output when the estimated context
  exceeds ~85 % of the window (rough 3-chars-per-token estimate); there is no
  summarisation/compaction.
- "Allow for this chat" is kept in memory only; it resets when Ycode restarts.
- The terminal tool runs Windows PowerShell 5.1 (`powershell.exe`); PowerShell 7
  isn't used even if installed.
- One response at a time per conversation; different conversations can run at once.

## Models & effort
- OpenAI's list-models endpoint has no capability data: models outside the built-in
  table start with effort control disabled until added to `providers/registry.rs`.
- DeepSeek has no documented way to change effort without touching the request
  parameters, so changing it mid-chat may cost a cache miss.
- Effort can't be changed mid-conversation for Claude with thinking turned off
  (Ycode always keeps adaptive thinking on for Claude 5.x).

## Platform
- Windows x64 only. No code-signing certificate: SmartScreen may warn on first install.
- The UI targets a minimum window of 900×600.
- Install, launch, restart and uninstall were verified locally (see TESTING.md), and
  the installer's update signature was verified against the embedded public key.
  An end-to-end in-app update has **not** been exercised yet: it needs two published
  GitHub releases. The GitHub Actions workflows have not run yet because the
  repository has no remote.

## Out of scope by design
Multi-agent, marketplace, built-in browser, IDE editor, accounts/cloud sync, voice,
image generation, MCP management, scheduling, dashboards.
