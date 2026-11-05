import { useState } from "react";
import { PanelLeftClose, PanelLeftOpen, Settings as SettingsIcon, SquarePen, Trash2 } from "lucide-react";
import { useStore } from "../lib/store";
import { useT } from "../lib/i18n";
import { ConfirmDialog } from "./ConfirmDialog";

export function Sidebar() {
  const t = useT();
  const conversations = useStore((s) => s.conversations);
  const currentId = useStore((s) => s.currentId);
  const view = useStore((s) => s.view);
  const running = useStore((s) => s.running);
  const collapsed = useStore((s) => s.settings?.sidebar_collapsed ?? false);
  const { newChat, openChat, deleteChat, setView, patchSettings } = useStore.getState();
  const [confirm, setConfirm] = useState<string | null>(null);

  if (collapsed) {
    return (
      <aside className="flex w-12 shrink-0 flex-col items-center gap-1 border-r border-line bg-sidebar py-3">
        <button type="button" title={t("sidebar.expand")} onClick={() => patchSettings({ sidebar_collapsed: false })} className="rounded-md p-2 text-muted hover:bg-hover hover:text-fg">
          <PanelLeftOpen size={17} />
        </button>
        <button type="button" title={t("sidebar.newChat")} onClick={newChat} className="rounded-md p-2 text-muted hover:bg-hover hover:text-fg">
          <SquarePen size={17} />
        </button>
        <div className="flex-1" />
        <button type="button" title={t("sidebar.settings")} onClick={() => setView("settings")} className="rounded-md p-2 text-muted hover:bg-hover hover:text-fg">
          <SettingsIcon size={17} />
        </button>
      </aside>
    );
  }

  return (
    <aside className="flex w-[248px] shrink-0 flex-col border-r border-line bg-sidebar">
      <div className="flex items-center justify-between px-4 pt-4 pb-2">
        <div className="flex items-center gap-2">
          <span className="flex h-6 w-6 items-center justify-center rounded-md bg-accent text-[13px] font-bold text-accent-fg">Y</span>
          <span className="text-[15px] font-semibold tracking-tight">{t("app.name")}</span>
        </div>
        <button type="button" title={t("sidebar.collapse")} onClick={() => patchSettings({ sidebar_collapsed: true })} className="rounded-md p-1 text-faint hover:bg-hover hover:text-fg">
          <PanelLeftClose size={16} />
        </button>
      </div>
      <div className="px-2">
        <button
          type="button"
          onClick={newChat}
          className={`flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-[13px] hover:bg-hover ${currentId === null && view === "chat" ? "bg-hover" : ""}`}
        >
          <SquarePen size={15} className="text-muted" />
          {t("sidebar.newChat")}
        </button>
      </div>
      <div className="mt-3 px-4 pb-1 text-[11px] font-medium tracking-wide text-faint uppercase">{t("sidebar.history")}</div>
      <nav className="min-h-0 flex-1 overflow-y-auto px-2 pb-2">
        {conversations.length === 0 && <div className="px-2.5 py-2 text-xs text-faint">{t("sidebar.empty")}</div>}
        {conversations.map((c) => (
          <div
            key={c.id}
            className={`group flex items-center rounded-md ${c.id === currentId && view === "chat" ? "bg-hover" : "hover:bg-hover/70"}`}
          >
            <button type="button" onClick={() => openChat(c.id)} className="min-w-0 flex-1 truncate px-2.5 py-1.5 text-left text-[13px]" title={c.title}>
              {running[c.id] && <span className="pulse-dot mr-1.5 inline-block h-1.5 w-1.5 rounded-full bg-fg align-middle" />}
              {c.title || t("sidebar.untitled")}
            </button>
            <button
              type="button"
              title={t("sidebar.delete")}
              onClick={() => setConfirm(c.id)}
              className="mr-1 rounded p-1 text-faint opacity-0 group-hover:opacity-100 hover:text-danger focus:opacity-100"
            >
              <Trash2 size={13} />
            </button>
          </div>
        ))}
      </nav>
      <div className="border-t border-line p-2">
        <button
          type="button"
          onClick={() => setView("settings")}
          className={`flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-[13px] hover:bg-hover ${view === "settings" ? "bg-hover" : ""}`}
        >
          <SettingsIcon size={15} className="text-muted" />
          {t("sidebar.settings")}
        </button>
      </div>
      {confirm && (
        <ConfirmDialog
          message={t("confirm.deleteChat")}
          onCancel={() => setConfirm(null)}
          onConfirm={() => {
            deleteChat(confirm);
            setConfirm(null);
          }}
        />
      )}
    </aside>
  );
}
