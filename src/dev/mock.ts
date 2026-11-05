// Dev-only: lets `npm run dev` render in a normal browser (no Tauri runtime) with
// fake data, for UI work and screenshots. Never imported in production builds.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import type { Bootstrap, Message } from "../lib/types";

const now = Date.now();
const boot: Bootstrap = {
  version: "0.1.0-dev",
  schema_version: 1,
  settings: {
    language: null,
    theme: "system",
    provider: "anthropic",
    mode: "coding",
    models: {},
    efforts: {},
    base_urls: {},
    manual_models: {},
    workspace: "C:\\Users\\dev\\projects\\demo-app",
    recent_workspaces: [],
    disabled_skills: [],
    update_repo: "",
    check_updates_on_start: false,
    claude_fallbacks: true,
    sidebar_collapsed: false,
    max_steps: 40,
  },
  providers: [
    { id: "openai", has_key: true, base_url: "", default_base_url: "https://api.openai.com/v1" },
    { id: "deepseek", has_key: false, base_url: "", default_base_url: "https://api.deepseek.com" },
    { id: "anthropic", has_key: true, base_url: "", default_base_url: "https://api.anthropic.com" },
  ],
  models: [
    { provider: "openai", id: "gpt-6-astra", display_name: "GPT-6 Astra", efforts: ["low", "medium", "high", "xhigh", "max"], default_effort: "medium", effort_update: "openai_item", tools: true, vision: true, context_window: 1050000, max_output: 128000, min_cache_tokens: 1024, recommended: ["coding"], adaptive_thinking: false, server_fallbacks: false, source: "builtin" },
    { provider: "openai", id: "gpt-6.1-terra", display_name: "GPT-6.1 Terra", efforts: ["none", "low", "medium", "high", "xhigh", "max"], default_effort: "medium", effort_update: "openai_item", tools: true, vision: true, context_window: 1050000, max_output: 128000, min_cache_tokens: 1024, recommended: ["office"], adaptive_thinking: false, server_fallbacks: false, source: "builtin" },
    { provider: "deepseek", id: "deepseek-v4-pro", display_name: "DeepSeek V4 Pro", efforts: ["none", "low", "high", "max"], default_effort: "high", effort_update: "param", tools: true, vision: false, context_window: 1048576, max_output: 393216, min_cache_tokens: 0, recommended: ["coding"], adaptive_thinking: false, server_fallbacks: false, source: "builtin" },
    { provider: "anthropic", id: "claude-opus-5-5", display_name: "Claude Opus 5.5", efforts: ["low", "medium", "high", "xhigh", "max"], default_effort: "medium", effort_update: "anthropic_system_message", tools: true, vision: true, context_window: 1000000, max_output: 128000, min_cache_tokens: 512, recommended: ["coding"], adaptive_thinking: true, server_fallbacks: true, source: "builtin" },
    { provider: "anthropic", id: "claude-sonnet-5-5", display_name: "Claude Sonnet 5.5", efforts: ["low", "medium", "high", "xhigh", "max"], default_effort: "high", effort_update: "anthropic_system_message", tools: true, vision: true, context_window: 1000000, max_output: 128000, min_cache_tokens: 512, recommended: ["office"], adaptive_thinking: true, server_fallbacks: true, source: "builtin" },
  ],
  conversations: [
    { id: "c1", title: "Fix flaky login test", mode: "coding", created_at: now, updated_at: now },
    { id: "c2", title: "Translate release notes to English", mode: "office", created_at: now, updated_at: now },
  ],
  skills: [{ name: "release-notes", description: "Draft release notes from a git log. Use when preparing a release.", enabled: true, path: "", has_scripts: false, warnings: [] }],
};

const convo: Message[] = [
  { id: "u1", role: "user", text: "The login test fails about one run in five. Can you find out why?", status: "complete", created_at: now },
  {
    id: "a1",
    role: "assistant",
    text: "I'll look at the test and the session helper.",
    reasoning: "Search for the test first, then read the helper it uses.",
    tool_calls: [
      { id: "t1", name: "search_files", arguments: { pattern: "login", glob: "*.test.ts" } },
      { id: "t2", name: "edit_file", arguments: { path: "src/session.ts", old_string: "setTimeout(done, 10)", new_string: "await flush()" } },
    ],
    status: "complete",
    provider: "anthropic",
    model: "claude-opus-5-5",
    created_at: now,
  },
  {
    id: "r1",
    role: "tool",
    text: "",
    tool_results: [
      { call_id: "t1", name: "search_files", content: "tests/login.test.ts:12: it('logs in', async () => {", is_error: false, status: "success" },
      { call_id: "t2", name: "edit_file", content: "Wrote src/session.ts", is_error: false, status: "success", diff: "--- a/session.ts\n+++ b/session.ts\n@@ -8,3 +8,3 @@\n export async function save() {\n-  setTimeout(done, 10)\n+  await flush()\n }" },
    ],
    status: "complete",
    created_at: now,
  },
  {
    id: "a2",
    role: "assistant",
    text: "The test raced a **10 ms timer** in `session.ts`. I replaced it with an awaited flush:\n\n```ts\nexport async function save() {\n  await flush();\n}\n```\n\n- Root cause: timer vs. assertion ordering\n- Verified: 20/20 local runs pass",
    status: "complete",
    usage: { input_tokens: 18240, output_tokens: 412, cached_input_tokens: 16384, cache_write_tokens: 1200, eligible_input_tokens: 18240 },
    provider: "anthropic",
    model: "claude-opus-5-5",
    created_at: now,
  },
];

export function installMock() {
  mockWindows("main");
  mockIPC((cmd, args) => {
    switch (cmd) {
      case "bootstrap":
        return boot;
      case "update_settings":
        Object.assign(boot.settings, (args as { patch: object }).patch);
        return boot.settings;
      case "get_conversation":
        return { ...boot.conversations[0], messages: convo, usage: { input_tokens: 30000, output_tokens: 900, cached_input_tokens: 24000, eligible_input_tokens: 30000 } };
      case "list_conversations":
        return boot.conversations;
      case "list_skills":
        return boot.skills;
      default:
        return null;
    }
  });
}
