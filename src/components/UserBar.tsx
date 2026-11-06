import { useState } from "react";
import { ArrowDownCircle, Check, Loader2, Settings as SettingsIcon, UserRound } from "lucide-react";
import { relaunch } from "@tauri-apps/plugin-process";
import { displayName, useStore } from "../lib/store";
import { useT } from "../lib/i18n";
import { channelLabel, displayVersion } from "../lib/format";
import { Avatar } from "./Avatar";
import { Menu, MenuItem, useDropdown } from "./Popover";
import { ProfileDialog } from "./ProfileDialog";

/** Update popover: check, show notes, download + install, restart. */
function UpdatePanel() {
  const t = useT();
  const u = useStore((s) => s.update);
  const version = useStore((s) => s.version);
  const channel = useStore((s) => s.channel);
  const { checkUpdate, installUpdate } = useStore.getState();
  if (channelLabel(channel)) {
    return (
      <div className="w-full p-2 text-[12.5px]">
        <div className="mb-2 flex items-center justify-between">
          <span className="font-medium">{t("update.button")}</span>
          <span className="text-[11px] text-faint">{t("about.version", { v: displayVersion(version) })}</span>
        </div>
        <p className="text-xs leading-relaxed text-muted">{t("update.preBuild", { channel: channelLabel(channel)! })}</p>
      </div>
    );
  }
  const btn = "inline-flex items-center gap-1.5 rounded-md border border-line px-3 py-1.5 text-xs hover:bg-hover disabled:opacity-50";
  return (
    <div className="w-full p-2 text-[12.5px]">
      <div className="mb-2 flex items-center justify-between">
        <span className="font-medium">{t("update.button")}</span>
        <span className="text-[11px] text-faint">{t("about.version", { v: displayVersion(version) })}</span>
      </div>
      {u.status === "checking" && (
        <div className="flex items-center gap-1.5 text-muted">
          <Loader2 size={13} className="animate-spin" />
          {t("update.checking")}
        </div>
      )}
      {u.status === "error" && <div className="mb-2 text-xs break-words text-danger">{u.error}</div>}
      {u.status !== "checking" && u.info && !u.info.available && (
        <div className="flex items-center gap-1.5 text-ok">
          <Check size={13} />
          {t("updates.none", { version: displayVersion(u.info.current_version) })}
        </div>
      )}
      {u.info?.available && (
        <div>
          <div className="mb-1 font-medium">{t("updates.available", { version: displayVersion(u.info.version) })}</div>
          {u.info.notes && <pre className="mb-2 max-h-40 overflow-auto font-sans text-xs whitespace-pre-wrap text-muted">{u.info.notes}</pre>}
          {u.status === "done" ? (
            <div className="space-y-2 text-xs">
              <div>{t("updates.restart")}</div>
              <button type="button" className={btn} onClick={() => relaunch()}>
                {t("updates.restartNow")}
              </button>
            </div>
          ) : (
            <button type="button" className={btn} disabled={u.status === "downloading"} onClick={() => installUpdate()}>
              {u.status === "downloading" ? <Loader2 size={13} className="animate-spin" /> : <ArrowDownCircle size={13} />}
              {u.status === "downloading" ? t("updates.downloading", { pct: u.progress }) : t("updates.install")}
            </button>
          )}
        </div>
      )}
      {u.status !== "checking" && u.status !== "downloading" && u.status !== "done" && !u.info?.available && (
        <button type="button" className={`${btn} mt-2`} onClick={() => checkUpdate()}>
          {t("update.check")}
        </button>
      )}
    </div>
  );
}

/**
 * Bottom of the sidebar: avatar + name (menu: Profile, Settings) and, to its
 * right, the update button. `compact` is the collapsed-sidebar variant.
 */
export function UserBar({ compact = false }: { compact?: boolean }) {
  const t = useT();
  const name = useStore((s) => displayName(s));
  const avatar = useStore((s) => s.avatar);
  const view = useStore((s) => s.view);
  const updateAvailable = useStore((s) => !!s.update.info?.available);
  const newVersion = useStore((s) => s.update.info?.version ?? null);
  const hasInfo = useStore((s) => !!s.update.info);
  const channel = useStore((s) => s.channel);
  const version = useStore((s) => s.version);
  const { setView, checkUpdate } = useStore.getState();
  const user = useDropdown();
  const upd = useDropdown();
  const [profile, setProfile] = useState(false);

  const menuPos = compact ? "bottom-0 left-full ml-2 w-44" : "left-0 w-52";
  const updPos = compact ? "bottom-0 left-full ml-2 w-72" : "left-0 right-0";

  return (
    <div className={`relative flex items-center gap-1 ${compact ? "flex-col" : "border-t border-line p-2"}`}>
      <div className={`relative ${compact ? "" : "min-w-0 flex-1"}`} ref={user.ref}>
        <button
          type="button"
          onClick={() => user.setOpen((o) => !o)}
          title={name}
          className={`flex items-center gap-2 rounded-md hover:bg-hover ${compact ? "p-1.5" : "w-full px-2 py-1.5"} ${view === "settings" ? "bg-hover" : ""}`}
        >
          <Avatar src={avatar} name={name} size={compact ? 26 : 28} />
          {!compact && <span className="min-w-0 flex-1 truncate text-left text-[13px] font-medium">{name}</span>}
        </button>
        {user.open && (
          <Menu up className={menuPos}>
            <MenuItem
              onClick={() => {
                user.setOpen(false);
                setProfile(true);
              }}
            >
              <UserRound size={14} className="text-muted" />
              {t("user.profile")}
            </MenuItem>
            <MenuItem
              onClick={() => {
                user.setOpen(false);
                setView("settings");
              }}
            >
              <SettingsIcon size={14} className="text-muted" />
              {t("user.settings")}
            </MenuItem>
          </Menu>
        )}
      </div>
      <div className={compact ? "relative" : "flex items-center gap-1"} ref={upd.ref}>
        {!compact && channelLabel(channel) && (
          <span className="rounded border border-warn/50 px-1 text-[10px] font-semibold tracking-wide text-warn" title={displayVersion(version)}>
            {channelLabel(channel)}
          </span>
        )}
        <button
          type="button"
          title={updateAvailable ? t("update.new", { version: displayVersion(newVersion) }) : t("update.button")}
          onClick={() => {
            const opening = !upd.open;
            upd.setOpen(opening);
            if (opening && !hasInfo && !channelLabel(channel)) checkUpdate();
          }}
          className={`flex shrink-0 items-center gap-1 rounded-md text-xs ${
            updateAvailable ? "bg-accent px-2 py-1.5 font-medium text-accent-fg" : "p-1.5 text-muted hover:bg-hover hover:text-fg"
          }`}
        >
          <ArrowDownCircle size={16} />
          {updateAvailable && !compact && <span>{displayVersion(newVersion)}</span>}
        </button>
        {upd.open && (
          <Menu up className={updPos}>
            <UpdatePanel />
          </Menu>
        )}
      </div>
      {profile && <ProfileDialog onClose={() => setProfile(false)} />}
    </div>
  );
}
