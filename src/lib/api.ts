import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  AgentEvent,
  Attachment,
  Bootstrap,
  Conversation,
  ConversationGroup,
  ConversationSummary,
  Effort,
  Mode,
  ModelCaps,
  ProviderId,
  ProviderInfo,
  Settings,
  SkillMeta,
  UpdateInfo,
} from "./types";

export const api = {
  bootstrap: () => invoke<Bootstrap>("bootstrap"),
  updateSettings: (patch: Partial<Settings>) => invoke<Settings>("update_settings", { patch }),
  setApiKey: (providerId: ProviderId, key: string) => invoke<ProviderInfo[]>("set_api_key", { providerId, key }),
  deleteApiKey: (providerId: ProviderId) => invoke<ProviderInfo[]>("delete_api_key", { providerId }),
  setBaseUrl: (providerId: ProviderId, url: string) => invoke<ProviderInfo[]>("set_base_url", { providerId, url }),
  refreshModels: (providerId: ProviderId) => invoke<{ models: ModelCaps[]; returned: number }>("refresh_models", { providerId }),
  addManualModel: (providerId: ProviderId, modelId: string) => invoke<ModelCaps[]>("add_manual_model", { providerId, modelId }),
  removeManualModel: (providerId: ProviderId, modelId: string) => invoke<ModelCaps[]>("remove_manual_model", { providerId, modelId }),
  listConversations: () => invoke<ConversationSummary[]>("list_conversations"),
  getConversation: (id: string) => invoke<Conversation>("get_conversation", { id }),
  deleteConversation: (id: string) => invoke<void>("delete_conversation", { id }),
  sendMessage: (
    params: {
      conversation_id: string;
      text: string | null;
      attachments: Attachment[];
      provider: ProviderId;
      model: string;
      effort: Effort | null;
      mode: Mode;
      group_id?: string | null;
    },
    onEvent: (e: AgentEvent) => void,
  ) => {
    const ch = new Channel<AgentEvent>();
    ch.onmessage = onEvent;
    return invoke<void>("send_message", { params, onEvent: ch });
  },
  stopRun: (conversationId: string) => invoke<void>("stop_run", { conversationId }),
  respondApproval: (requestId: string, decision: "allow" | "allow_session" | "deny") =>
    invoke<void>("respond_approval", { requestId, decision }),
  setWorkspace: (path: string | null) => invoke<Settings>("set_workspace", { path }),
  inspectAttachment: (path: string) => invoke<Attachment>("inspect_attachment", { path }),
  listSkills: () => invoke<SkillMeta[]>("list_skills"),
  installSkill: (path: string, overwrite: boolean) => invoke<SkillMeta[]>("install_skill", { path, overwrite }),
  setSkillEnabled: (name: string, enabled: boolean) => invoke<SkillMeta[]>("set_skill_enabled", { name, enabled }),
  deleteSkill: (name: string) => invoke<SkillMeta[]>("delete_skill", { name }),
  validateSkill: (path: string) => invoke<SkillMeta>("validate_skill", { path }),
  checkUpdate: () => invoke<UpdateInfo>("check_update"),
  installUpdate: (onProgress: (p: { downloaded: number; total?: number | null }) => void) => {
    const ch = new Channel<{ downloaded: number; total?: number | null }>();
    ch.onmessage = onProgress;
    return invoke<void>("install_update", { onProgress: ch });
  },
  dataFolder: () => invoke<string>("open_data_folder"),
  createGroup: (name: string) => invoke<ConversationGroup>("create_group", { name }),
  renameGroup: (id: string, name: string) => invoke<void>("rename_group", { id, name }),
  setGroupCollapsed: (id: string, collapsed: boolean) => invoke<void>("set_group_collapsed", { id, collapsed }),
  deleteGroup: (id: string) => invoke<void>("delete_group", { id }),
  moveConversation: (id: string, groupId: string | null) => invoke<void>("move_conversation", { id, groupId }),
  readImage: (id: string) => invoke<string>("read_image", { id }),
  saveImageAs: (id: string, dest: string) => invoke<void>("save_image_as", { id, dest }),
};
