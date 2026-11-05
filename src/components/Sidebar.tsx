import { useRef, useState } from "react";
import { ChevronRight, FolderInput, FolderPlus, MoreHorizontal, PanelLeftClose, PanelLeftOpen, Pencil, Plus, Settings as SettingsIcon, SquarePen, Trash2 } from "lucide-react";
import { useStore } from "../lib/store";
import { useT } from "../lib/i18n";
import type { ConversationGroup, ConversationSummary } from "../lib/types";
import { ConfirmDialog } from "./ConfirmDialog";
import { Logo } from "./Logo";
import { Menu, MenuItem, useDropdown } from "./Popover";

function NameInput({ initial, onDone }: { initial: string; onDone: (name: string | null) => void }) {
  const t = useT();
  const [v, setV] = useState(initial);
  // Enter/Escape unmount the input, which also fires blur: submit only once.
  const done = useRef(false);
  const finish = (name: string | null) => {
    if (done.current) return;
    done.current = true;
    onDone(name);
  };
  return (
    <input
      autoFocus
      value={v}
      maxLength={60}
      placeholder={t("groups.namePlaceholder")}
      onChange={(e) => setV(e.target.value)}
      onBlur={() => finish(v.trim() ? v : null)}
      onKeyDown={(e) => {
        if (e.key === "Enter") finish(v.trim() ? v : null);
        if (e.key === "Escape") finish(null);
      }}
      className="mx-2 my-0.5 w-[calc(100%-1rem)] rounded-md border border-line-strong bg-bg px-2 py-1 text-[13px] outline-none"
    />
  );
}

function ChatRow({ c, onDelete }: { c: ConversationSummary; onDelete: (id: string) => void }) {
  const t = useT();
  const active = useStore((s) => s.currentId === c.id && s.view === "chat");
  const running = useStore((s) => !!s.running[c.id]);
  const groups = useStore((s) => s.groups);
  const { openChat, moveChat } = useStore.getState();
  const { open, setOpen, ref } = useDropdown();
  return (
    <div className={`group relative flex items-center rounded-md ${active ? "bg-hover" : "hover:bg-hover/70"}`} ref={ref}>
      <button type="button" onClick={() => openChat(c.id)} className="min-w-0 flex-1 truncate px-2.5 py-1.5 text-left text-[13px]" title={c.title}>
        {running && <span className="pulse-dot mr-1.5 inline-block h-1.5 w-1.5 rounded-full bg-fg align-middle" />}
        {c.title || t("sidebar.untitled")}
      </button>
      <button
        type="button"
        title={t("sidebar.more")}
        onClick={() => setOpen((o) => !o)}
        className={`mr-1 rounded p-1 text-faint hover:text-fg ${open ? "opacity-100" : "opacity-0 group-hover:opacity-100 focus:opacity-100"}`}
      >
        <MoreHorizontal size={14} />
      </button>
      {open && (
        <Menu className="right-0 w-52">
          <div className="px-2 pt-1 pb-1 text-[11px] text-faint">{t("groups.moveTo")}</div>
          {groups.map((g) => (
            <MenuItem
              key={g.id}
              active={c.group_id === g.id}
              onClick={() => {
                setOpen(false);
                moveChat(c.id, g.id);
              }}
            >
              <FolderInput size={14} className="text-muted" />
              <span className="truncate">{g.name}</span>
            </MenuItem>
          ))}
          {c.group_id && (
            <MenuItem
              onClick={() => {
                setOpen(false);
                moveChat(c.id, null);
              }}
            >
              <FolderInput size={14} className="text-muted" />
              {t("groups.ungrouped")}
            </MenuItem>
          )}
          <div className="mt-1 border-t border-line pt-1">
            <MenuItem
              danger
              onClick={() => {
                setOpen(false);
                onDelete(c.id);
              }}
            >
              <Trash2 size={14} />
              {t("sidebar.delete")}
            </MenuItem>
          </div>
        </Menu>
      )}
    </div>
  );
}

function GroupSection({ g, chats, onDeleteChat, onDeleteGroup }: { g: ConversationGroup; chats: ConversationSummary[]; onDeleteChat: (id: string) => void; onDeleteGroup: (g: ConversationGroup) => void }) {
  const t = useT();
  const { toggleGroup, renameGroup, newChat } = useStore.getState();
  const draftHere = useStore((s) => s.currentId === null && s.draftGroupId === g.id && s.view === "chat");
  const [renaming, setRenaming] = useState(false);
  const { open, setOpen, ref } = useDropdown();
  return (
    <div className="mb-1">
      {renaming ? (
        <NameInput
          initial={g.name}
          onDone={(n) => {
            setRenaming(false);
            if (n && n !== g.name) renameGroup(g.id, n);
          }}
        />
      ) : (
        <div className="group relative flex items-center rounded-md hover:bg-hover/70" ref={ref}>
          <button type="button" onClick={() => toggleGroup(g.id)} className="flex min-w-0 flex-1 items-center gap-1 px-1.5 py-1.5 text-left text-[12.5px] font-medium text-muted">
            <ChevronRight size={14} className={`shrink-0 transition-transform ${g.collapsed ? "" : "rotate-90"}`} />
            <span className="truncate">{g.name}</span>
            <span className="ml-1 text-[11px] font-normal text-faint">{chats.length}</span>
          </button>
          <button type="button" title={t("groups.newChatIn")} onClick={() => newChat(g.id)} className={`rounded p-1 text-faint hover:text-fg ${draftHere ? "text-fg" : "opacity-0 group-hover:opacity-100"}`}>
            <Plus size={14} />
          </button>
          <button type="button" title={t("sidebar.more")} onClick={() => setOpen((o) => !o)} className={`mr-1 rounded p-1 text-faint hover:text-fg ${open ? "" : "opacity-0 group-hover:opacity-100"}`}>
            <MoreHorizontal size={14} />
          </button>
          {open && (
            <Menu className="right-0 w-44">
              <MenuItem
                onClick={() => {
                  setOpen(false);
                  setRenaming(true);
                }}
              >
                <Pencil size={14} className="text-muted" />
                {t("groups.rename")}
              </MenuItem>
              <MenuItem
                danger
                onClick={() => {
                  setOpen(false);
                  onDeleteGroup(g);
                }}
              >
                <Trash2 size={14} />
                {t("groups.delete")}
              </MenuItem>
            </Menu>
          )}
        </div>
      )}
      {!g.collapsed && (
        <div className="ml-3 border-l border-line pl-1">
          {chats.length === 0 && <div className="px-2.5 py-1 text-[12px] text-faint">{t("groups.empty")}</div>}
          {chats.map((c) => (
            <ChatRow key={c.id} c={c} onDelete={onDeleteChat} />
          ))}
        </div>
      )}
    </div>
  );
}

export function Sidebar() {
  const t = useT();
  const conversations = useStore((s) => s.conversations);
  const groups = useStore((s) => s.groups);
  const view = useStore((s) => s.view);
  const newChatActive = useStore((s) => s.currentId === null && s.draftGroupId === null && s.view === "chat");
  const collapsed = useStore((s) => s.settings?.sidebar_collapsed ?? false);
  const { newChat, deleteChat, deleteGroup, createGroup, setView, patchSettings } = useStore.getState();
  const [confirmChat, setConfirmChat] = useState<string | null>(null);
  const [confirmGroup, setConfirmGroup] = useState<ConversationGroup | null>(null);
  const [creating, setCreating] = useState(false);

  if (collapsed) {
    return (
      <aside className="flex w-14 shrink-0 flex-col items-center gap-1 border-r border-line bg-sidebar py-3">
        <Logo size={30} className="mb-2" />
        <button type="button" title={t("sidebar.expand")} onClick={() => patchSettings({ sidebar_collapsed: false })} className="rounded-md p-2 text-muted hover:bg-hover hover:text-fg">
          <PanelLeftOpen size={17} />
        </button>
        <button type="button" title={t("sidebar.newChat")} onClick={() => newChat()} className="rounded-md p-2 text-muted hover:bg-hover hover:text-fg">
          <SquarePen size={17} />
        </button>
        <div className="flex-1" />
        <button type="button" title={t("sidebar.settings")} onClick={() => setView("settings")} className="rounded-md p-2 text-muted hover:bg-hover hover:text-fg">
          <SettingsIcon size={17} />
        </button>
      </aside>
    );
  }

  const known = new Set(groups.map((g) => g.id));
  const ungrouped = conversations.filter((c) => !c.group_id || !known.has(c.group_id));

  return (
    <aside className="flex w-[260px] shrink-0 flex-col border-r border-line bg-sidebar">
      <div className="flex items-center justify-between px-4 pt-4 pb-3">
        <div className="flex items-center gap-2.5">
          <Logo size={32} />
          <span className="text-[19px] font-semibold tracking-tight">{t("app.name")}</span>
        </div>
        <button type="button" title={t("sidebar.collapse")} onClick={() => patchSettings({ sidebar_collapsed: true })} className="rounded-md p-1 text-faint hover:bg-hover hover:text-fg">
          <PanelLeftClose size={17} />
        </button>
      </div>
      <div className="px-2">
        <button type="button" onClick={() => newChat()} className={`flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-[13px] hover:bg-hover ${newChatActive ? "bg-hover" : ""}`}>
          <SquarePen size={15} className="text-muted" />
          {t("sidebar.newChat")}
        </button>
      </div>
      <nav className="mt-2 min-h-0 flex-1 overflow-y-auto px-2 pb-2">
        <div className="group flex items-center justify-between px-2 pt-1 pb-1">
          <span className="text-[11px] font-medium tracking-wide text-faint uppercase">{t("sidebar.history")}</span>
          <button type="button" title={t("groups.new")} onClick={() => setCreating(true)} className="rounded p-0.5 text-faint hover:bg-hover hover:text-fg">
            <FolderPlus size={14} />
          </button>
        </div>
        {creating && (
          <NameInput
            initial=""
            onDone={(n) => {
              setCreating(false);
              if (n) createGroup(n);
            }}
          />
        )}
        {groups.map((g) => (
          <GroupSection key={g.id} g={g} chats={conversations.filter((c) => c.group_id === g.id)} onDeleteChat={setConfirmChat} onDeleteGroup={setConfirmGroup} />
        ))}
        {groups.length > 0 && ungrouped.length > 0 && <div className="mx-2 my-1.5 border-t border-line" />}
        {conversations.length === 0 && groups.length === 0 && <div className="px-2.5 py-2 text-xs text-faint">{t("sidebar.empty")}</div>}
        {ungrouped.map((c) => (
          <ChatRow key={c.id} c={c} onDelete={setConfirmChat} />
        ))}
      </nav>
      <div className="border-t border-line p-2">
        <button type="button" onClick={() => setView("settings")} className={`flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-[13px] hover:bg-hover ${view === "settings" ? "bg-hover" : ""}`}>
          <SettingsIcon size={15} className="text-muted" />
          {t("sidebar.settings")}
        </button>
      </div>
      {confirmChat && (
        <ConfirmDialog
          message={t("confirm.deleteChat")}
          onCancel={() => setConfirmChat(null)}
          onConfirm={() => {
            deleteChat(confirmChat);
            setConfirmChat(null);
          }}
        />
      )}
      {confirmGroup && (
        <ConfirmDialog
          message={t("groups.confirmDelete", { name: confirmGroup.name })}
          onCancel={() => setConfirmGroup(null)}
          onConfirm={() => {
            deleteGroup(confirmGroup.id);
            setConfirmGroup(null);
          }}
        />
      )}
    </aside>
  );
}
