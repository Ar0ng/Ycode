import { create } from "zustand";
import { api } from "./api";
import { clampEffort, defaultModel, effortKey, findCaps } from "./effort";
import { detectLang, type Lang } from "./i18n";
import type {
  AgentEvent,
  AppError,
  ApprovalPrompt,
  Attachment,
  ConversationSummary,
  Effort,
  Message,
  Mode,
  ModelCaps,
  ProviderId,
  ProviderInfo,
  Settings,
  SkillMeta,
  ToolStatus,
  Usage,
} from "./types";

export interface Notice {
  id: number;
  code: string;
  detail: string;
}

interface ToolState {
  status: ToolStatus;
  approval?: ApprovalPrompt;
}

interface State {
  ready: boolean;
  bootError: string | null;
  version: string;
  lang: Lang;
  settings: Settings | null;
  providers: ProviderInfo[];
  models: ModelCaps[];
  conversations: ConversationSummary[];
  skills: SkillMeta[];
  view: "chat" | "settings";
  currentId: string | null;
  draftId: string;
  messages: Message[];
  loadingConversation: boolean;
  running: Record<string, boolean>;
  tools: Record<string, ToolState>;
  convUsage: Usage | null;
  lastUsage: Usage | null;
  notices: Notice[];
  lastError: AppError | null;
}

interface Actions {
  init(): Promise<void>;
  newChat(): void;
  openChat(id: string): Promise<void>;
  deleteChat(id: string): Promise<void>;
  send(text: string, attachments: Attachment[]): Promise<void>;
  retry(): Promise<void>;
  stop(): void;
  approve(requestId: string, decision: "allow" | "allow_session" | "deny"): Promise<void>;
  patchSettings(p: Partial<Settings>): Promise<void>;
  setMode(m: Mode): void;
  selectModel(provider: ProviderId, model: string): void;
  setEffort(e: Effort): void;
  setWorkspace(path: string | null): Promise<void>;
  setView(v: "chat" | "settings"): void;
  dismissNotice(id: number): void;
  addNotice(code: string, detail?: string): void;
  setModels(m: ModelCaps[]): void;
  setProviders(p: ProviderInfo[]): void;
  setSkills(s: SkillMeta[]): void;
}

const uuid = () => crypto.randomUUID();
let noticeSeq = 1;

// Streaming deltas are buffered and flushed once per animation frame so a fast
// token stream doesn't trigger a React render per token.
const pending = new Map<string, { text: string; reasoning: string }>();
let frame = 0;

export const useStore = create<State & Actions>((set, get) => {
  const flush = () => {
    frame = 0;
    if (pending.size === 0) return;
    const batch = new Map(pending);
    pending.clear();
    set((s) => ({
      messages: s.messages.map((m) => {
        const d = batch.get(m.id);
        if (!d) return m;
        return { ...m, text: m.text + d.text, reasoning: d.reasoning ? (m.reasoning ?? "") + d.reasoning : m.reasoning };
      }),
    }));
  };

  const upsertMessage = (msg: Message) =>
    set((s) => {
      const i = s.messages.findIndex((m) => m.id === msg.id);
      if (i === -1) return { messages: [...s.messages, msg] };
      const copy = s.messages.slice();
      // A persisted message replaces any streamed partial state.
      pending.delete(msg.id);
      copy[i] = msg;
      return { messages: copy };
    });

  const handle = (convId: string) => (e: AgentEvent) => {
    const visible = () => get().currentId === convId;
    switch (e.type) {
      case "message":
        if (visible()) upsertMessage(e.message);
        break;
      case "delta":
        if (!visible()) break;
        {
          const p = pending.get(e.message_id) ?? { text: "", reasoning: "" };
          if (e.kind === "text") p.text += e.text;
          else p.reasoning += e.text;
          pending.set(e.message_id, p);
          if (!frame) frame = requestAnimationFrame(flush);
        }
        break;
      case "tool_status":
        set((s) => ({ tools: { ...s.tools, [e.call_id]: { status: e.status, approval: e.status === "pending" ? e.approval : undefined } } }));
        break;
      case "usage":
        if (visible()) set({ lastUsage: e.turn, convUsage: e.total });
        break;
      case "notice": {
        if (e.code === "provider") {
          if (e.detail === "fallback") get().addNotice("fallback");
          else if (e.detail.startsWith("retry:")) get().addNotice("retry", e.detail.split(":")[1]);
        } else get().addNotice(e.code, e.detail);
        break;
      }
      case "error":
        if (visible()) set({ lastError: e.error });
        break;
      case "title":
        set((s) => ({ conversations: s.conversations.map((c) => (c.id === e.conversation_id ? { ...c, title: e.title } : c)) }));
        break;
      case "finished":
        flush();
        set((s) => ({ running: { ...s.running, [convId]: false } }));
        api.listConversations().then((conversations) => set({ conversations })).catch(() => {});
        break;
    }
  };

  const currentSelection = () => {
    const s = get();
    const st = s.settings!;
    const provider = st.provider;
    const model = st.models[provider] ?? defaultModel(s.models, provider, st.mode) ?? "";
    const caps = findCaps(s.models, provider, model);
    const effort = clampEffort(caps, st.efforts[effortKey(provider, model)]);
    return { provider, model, caps, effort, mode: st.mode };
  };

  const run = async (convId: string, text: string | null, attachments: Attachment[]) => {
    const sel = currentSelection();
    set((s) => ({ running: { ...s.running, [convId]: true }, lastError: null }));
    try {
      await api.sendMessage(
        { conversation_id: convId, text, attachments, provider: sel.provider, model: sel.model, effort: sel.effort, mode: sel.mode },
        handle(convId),
      );
    } catch (err) {
      set((s) => ({ running: { ...s.running, [convId]: false }, lastError: err as AppError }));
    }
  };

  return {
    ready: false,
    bootError: null,
    version: "",
    lang: detectLang(),
    settings: null,
    providers: [],
    models: [],
    conversations: [],
    skills: [],
    view: "chat",
    currentId: null,
    draftId: uuid(),
    messages: [],
    loadingConversation: false,
    running: {},
    tools: {},
    convUsage: null,
    lastUsage: null,
    notices: [],
    lastError: null,

    async init() {
      try {
        const b = await api.bootstrap();
        let settings = b.settings;
        if (!settings.language) settings = await api.updateSettings({ language: detectLang() });
        set({
          ready: true,
          version: b.version,
          settings,
          lang: (settings.language as Lang) ?? detectLang(),
          providers: b.providers,
          models: b.models,
          conversations: b.conversations,
          skills: b.skills,
        });
        if (b.recovery_notice) get().addNotice("recovered", b.recovery_notice);
      } catch (e) {
        set({ bootError: (e as AppError)?.message ?? String(e) });
      }
    },

    newChat() {
      set({ currentId: null, draftId: uuid(), messages: [], tools: {}, convUsage: null, lastUsage: null, lastError: null, view: "chat" });
    },

    async openChat(id) {
      set({ view: "chat", currentId: id, loadingConversation: true, messages: [], tools: {}, lastError: null, lastUsage: null });
      try {
        const c = await api.getConversation(id);
        if (get().currentId !== id) return;
        const lastAssistant = [...c.messages].reverse().find((m) => m.role === "assistant" && m.usage);
        const err = c.messages.length && c.messages[c.messages.length - 1].status === "error" ? (c.messages[c.messages.length - 1].error ?? null) : null;
        set({ messages: c.messages, convUsage: c.usage ?? null, lastUsage: lastAssistant?.usage ?? null, loadingConversation: false, lastError: err });
      } catch (e) {
        set({ loadingConversation: false, lastError: e as AppError });
      }
    },

    async deleteChat(id) {
      await api.deleteConversation(id);
      set((s) => ({ conversations: s.conversations.filter((c) => c.id !== id) }));
      if (get().currentId === id) get().newChat();
    },

    async send(text, attachments) {
      const s = get();
      let convId = s.currentId;
      if (!convId) {
        convId = s.draftId;
        const now = Date.now();
        set((st) => ({
          currentId: convId,
          conversations: [{ id: convId!, title: "", mode: st.settings!.mode, created_at: now, updated_at: now, workspace: st.settings!.workspace }, ...st.conversations],
        }));
      }
      await run(convId, text, attachments);
    },

    async retry() {
      const id = get().currentId;
      if (!id) return;
      set((s) => ({ messages: s.messages.filter((m) => !(m.role === "assistant" && m.status === "error")) }));
      await run(id, null, []);
    },

    stop() {
      const id = get().currentId;
      if (id) api.stopRun(id).catch(() => {});
    },

    async approve(requestId, decision) {
      set((s) => {
        const tools = { ...s.tools };
        for (const [k, v] of Object.entries(tools)) if (v.approval?.request_id === requestId) tools[k] = { status: decision === "deny" ? "cancelled" : "running" };
        return { tools };
      });
      await api.respondApproval(requestId, decision).catch((e) => set({ lastError: e as AppError }));
    },

    async patchSettings(p) {
      const prev = get().settings;
      if (prev) set({ settings: { ...prev, ...p } });
      try {
        const settings = await api.updateSettings(p);
        set({ settings, lang: (settings.language as Lang) ?? get().lang });
      } catch (e) {
        set({ settings: prev, lastError: e as AppError });
      }
    },

    setMode(mode) {
      get().patchSettings({ mode });
    },

    selectModel(provider, model) {
      const st = get().settings!;
      get().patchSettings({ provider, models: { ...st.models, [provider]: model } });
    },

    setEffort(e) {
      const { provider, model } = currentSelection();
      const st = get().settings!;
      get().patchSettings({ efforts: { ...st.efforts, [effortKey(provider, model)]: e } });
    },

    async setWorkspace(path) {
      try {
        const settings = await api.setWorkspace(path);
        set({ settings });
      } catch (e) {
        set({ lastError: e as AppError });
      }
    },

    setView(view) {
      set({ view });
    },
    dismissNotice(id) {
      set((s) => ({ notices: s.notices.filter((n) => n.id !== id) }));
    },
    addNotice(code, detail = "") {
      const id = noticeSeq++;
      set((s) => ({ notices: [...s.notices.slice(-3), { id, code, detail }] }));
      setTimeout(() => get().dismissNotice(id), 8000);
    },
    setModels(models) {
      set({ models });
    },
    setProviders(providers) {
      set({ providers });
    },
    setSkills(skills) {
      set({ skills });
    },
  };
});

export function useSelection() {
  const settings = useStore((s) => s.settings);
  const models = useStore((s) => s.models);
  if (!settings) return null;
  const provider = settings.provider;
  const model = settings.models[provider] ?? defaultModel(models, provider, settings.mode) ?? "";
  const caps = findCaps(models, provider, model);
  const effort = clampEffort(caps, settings.efforts[effortKey(provider, model)]);
  return { provider, model, caps, effort, mode: settings.mode };
}
