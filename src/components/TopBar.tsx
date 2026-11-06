import { ChevronDown, Folder, FolderOpen, X } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useSelection, useStore } from "../lib/store";
import { useT } from "../lib/i18n";
import type { Mode } from "../lib/types";
import { Menu, MenuItem, useDropdown } from "./Popover";

const folderName = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;

function ModeSwitch() {
  const t = useT();
  const mode = useStore((s) => s.settings?.mode ?? "office");
  const setMode = useStore((s) => s.setMode);
  return (
    <div className="flex rounded-lg border border-line p-0.5 text-[13px]" role="tablist">
      {(["office", "coding"] as Mode[]).map((m) => (
        <button
          key={m}
          type="button"
          role="tab"
          aria-selected={mode === m}
          onClick={() => setMode(m)}
          className={`rounded-md px-3 py-1 ${mode === m ? "bg-hover font-medium text-fg" : "text-muted hover:text-fg"}`}
        >
          {t(`mode.${m}`)}
        </button>
      ))}
    </div>
  );
}

export function WorkspacePicker({ plain = false }: { plain?: boolean }) {
  const t = useT();
  const workspace = useStore((s) => s.settings?.workspace ?? null);
  const recent = useStore((s) => s.settings?.recent_workspaces);
  const setWorkspace = useStore((s) => s.setWorkspace);
  const { open, setOpen, ref } = useDropdown();

  const choose = async () => {
    setOpen(false);
    const dir = await openDialog({ directory: true, multiple: false, defaultPath: workspace ?? undefined });
    if (typeof dir === "string") setWorkspace(dir);
  };

  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        title={workspace ?? t("workspace.none")}
        className={
          plain
            ? `flex max-w-[300px] items-center gap-1.5 rounded-md px-2 py-1 text-[13px] font-semibold hover:bg-hover ${workspace ? "text-fg" : "text-muted"}`
            : `flex max-w-[260px] items-center gap-1.5 rounded-md border px-2.5 py-1 text-[13px] hover:bg-hover ${workspace ? "border-line text-fg" : "border-dashed border-line-strong text-muted"}`
        }
      >
        {workspace ? <FolderOpen size={14} /> : <Folder size={14} />}
        <span className="truncate">{workspace ? folderName(workspace) : t("workspace.choose")}</span>
        <ChevronDown size={13} className="shrink-0 text-faint" />
      </button>
      {open && (
        <Menu className={`${plain ? "left-0" : "right-0"} w-80`}>
          <div className="px-2 pt-1 pb-1.5 text-[11px] font-medium tracking-wide text-faint uppercase">{t("workspace.label")}</div>
          {workspace && <div className="mb-1 px-2 font-mono text-[11px] break-all text-muted">{workspace}</div>}
          <MenuItem onClick={choose}>
            <FolderOpen size={14} className="text-muted" />
            {t("workspace.choose")}
          </MenuItem>
          {recent && recent.filter((r) => r !== workspace).length > 0 && (
            <>
              <div className="mt-1 border-t border-line px-2 pt-1.5 pb-1 text-[11px] text-faint">{t("workspace.recent")}</div>
              {recent
                .filter((r) => r !== workspace)
                .slice(0, 6)
                .map((r) => (
                  <MenuItem
                    key={r}
                    onClick={() => {
                      setOpen(false);
                      setWorkspace(r);
                    }}
                  >
                    <Folder size={14} className="shrink-0 text-muted" />
                    <span className="min-w-0 flex-1 truncate" title={r}>
                      {folderName(r)}
                      <span className="ml-1.5 text-[11px] text-faint">{r}</span>
                    </span>
                  </MenuItem>
                ))}
            </>
          )}
          {workspace && (
            <div className="mt-1 border-t border-line pt-1">
              <MenuItem
                onClick={() => {
                  setOpen(false);
                  setWorkspace(null);
                }}
              >
                <X size={14} className="text-muted" />
                {t("composer.clearFolder")}
              </MenuItem>
            </div>
          )}
        </Menu>
      )}
    </div>
  );
}

/** Header of the main area: mode switch (new chat only) and workspace folder. */
export function TopBar({ isNew }: { isNew: boolean }) {
  const title = useStore((s) => (s.currentId ? s.conversations.find((c) => c.id === s.currentId)?.title : undefined));
  const mode = useSelection()?.mode ?? "office";
  const t = useT();
  return (
    <header className="flex h-12 shrink-0 items-center gap-3 px-4">
      {isNew ? (
        <ModeSwitch />
      ) : (
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate text-[13px] font-medium">{title || t("sidebar.untitled")}</span>
          <span className="flex shrink-0 items-center gap-1 rounded border border-line px-1.5 text-[11px] text-faint">
            {t(`mode.${mode}`)}
          </span>
        </div>
      )}
      <div className="ml-auto">
        <WorkspacePicker />
      </div>
    </header>
  );
}
