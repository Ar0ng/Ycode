// Mirrors of the Rust IPC types (src-tauri/src/model.rs, storage.rs, ...).

export type ProviderId = "openai" | "deepseek" | "anthropic";
export type Effort = "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max";
export type Mode = "office" | "coding";
export type Role = "user" | "assistant" | "tool" | "control";

export interface ToolCall {
  id: string;
  name: string;
  arguments: Record<string, unknown>;
}

export interface ToolResult {
  call_id: string;
  name: string;
  content: string;
  is_error: boolean;
  status: string;
  diff?: string;
  images?: string[];
}

export interface Usage {
  input_tokens: number;
  output_tokens: number;
  cached_input_tokens: number;
  cache_write_tokens?: number | null;
  reasoning_tokens?: number | null;
  eligible_input_tokens?: number | null;
}

export type AttachmentKind = "text" | "image" | "pdf" | "office";

export interface Attachment {
  id: string;
  name: string;
  mime: string;
  size: number;
  kind: AttachmentKind;
  text?: string;
}

export interface Control {
  type: "effort";
  provider: ProviderId;
  model: string;
  effort: Effort;
  in_band: boolean;
}

export interface Message {
  id: string;
  role: Role;
  text: string;
  reasoning?: string;
  tool_calls?: ToolCall[];
  tool_results?: ToolResult[];
  attachments?: Attachment[];
  control?: Control;
  usage?: Usage;
  provider?: ProviderId;
  model?: string;
  effort?: Effort;
  mode?: Mode;
  status: string;
  error?: AppError;
  created_at: number;
}

export interface ConversationSummary {
  id: string;
  title: string;
  mode: Mode;
  workspace?: string | null;
  created_at: number;
  updated_at: number;
  group_id?: string | null;
}

export interface ConversationGroup {
  id: string;
  name: string;
  sort: number;
  collapsed: boolean;
  created_at: number;
}

export interface Conversation extends ConversationSummary {
  messages: Message[];
  usage?: Usage | null;
}

export interface ModelCaps {
  provider: ProviderId;
  id: string;
  display_name: string;
  efforts: Effort[];
  default_effort?: Effort | null;
  effort_update: "param" | "openai_item" | "anthropic_system_message";
  tools: boolean;
  vision: boolean;
  context_window: number;
  max_output: number;
  min_cache_tokens: number;
  recommended: Mode[];
  adaptive_thinking: boolean;
  server_fallbacks: boolean;
  source: "builtin" | "remote" | "manual" | "unknown";
  available?: boolean | null;
  note?: string | null;
}

export interface Settings {
  language: string | null;
  theme: "system" | "light" | "dark";
  provider: ProviderId;
  mode: Mode;
  models: Record<string, string>;
  efforts: Record<string, Effort>;
  base_urls: Record<string, string>;
  manual_models: Record<string, string[]>;
  workspace: string | null;
  recent_workspaces: string[];
  disabled_skills: string[];
  update_repo: string;
  auto_update_repo: boolean;
  profile_name: string;
  auto_approve_edits: boolean;
  check_updates_on_start: boolean;
  claude_fallbacks: boolean;
  sidebar_collapsed: boolean;
  max_steps: number;
  image_generation: boolean;
  image_model: string;
  hide_legacy_models: boolean;
  hidden_models: string[];
}

export interface ProviderInfo {
  id: ProviderId;
  has_key: boolean;
  base_url: string;
  default_base_url: string;
  refreshed_at?: number | null;
}

export interface SkillMeta {
  name: string;
  description: string;
  license?: string | null;
  compatibility?: string | null;
  enabled: boolean;
  path: string;
  has_scripts: boolean;
  warnings: string[];
}

export interface Bootstrap {
  version: string;
  settings: Settings;
  providers: ProviderInfo[];
  models: ModelCaps[];
  conversations: ConversationSummary[];
  groups: ConversationGroup[];
  skills: SkillMeta[];
  recovery_notice?: string | null;
  os_user: string;
  avatar?: string | null;
  schema_version: number;
}

export type ProviderErrorKind =
  | "missing_key"
  | "invalid_key"
  | "insufficient_balance"
  | "model_not_found"
  | "model_no_access"
  | "rate_limited"
  | "overloaded"
  | "timeout"
  | "network"
  | "stream_interrupted"
  | "bad_request"
  | "refused"
  | "server"
  | "unknown";

export interface AppError {
  code: string;
  message: string;
  provider?: { kind: ProviderErrorKind; status?: number | null; detail: string; retryable: boolean };
}

export interface ApprovalPrompt {
  request_id: string;
  kind: "write_file" | "run_command" | "read_outside";
  target: string;
  diff?: string;
  warning?: string;
}

export type ToolStatus = "pending" | "running" | "success" | "failed" | "cancelled";

export type AgentEvent =
  | { type: "message"; message: Message }
  | { type: "delta"; message_id: string; kind: "text" | "reasoning"; text: string }
  | { type: "tool_status"; message_id: string; call_id: string; name: string; status: ToolStatus; approval?: ApprovalPrompt }
  | { type: "usage"; turn: Usage; total: Usage }
  | { type: "notice"; code: string; detail: string }
  | { type: "error"; message_id?: string | null; error: AppError }
  | { type: "title"; conversation_id: string; title: string }
  | { type: "finished"; stopped: boolean };

export interface UpdateInfo {
  available: boolean;
  current_version: string;
  version?: string | null;
  notes?: string | null;
  date?: string | null;
}

export const EFFORT_ORDER: Effort[] = ["none", "minimal", "low", "medium", "high", "xhigh", "max"];
