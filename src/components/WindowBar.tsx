import { useEffect, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, Minus, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "@tauri-apps/api/core";
import { useStore } from "../lib/store";
import { useT } from "../lib/i18n";

type Location = { view: "chat" | "settings"; id: string | null; group: string | null };
const current = (): Location => { const s = useStore.getState(); return { view: s.view, id: s.currentId, group: s.draftGroupId }; };
const same = (a: Location, b: Location) => a.view === b.view && a.id === b.id && (a.id !== null || a.group === b.group);

export function WindowBar() {
  const t = useT();
  const stack = useRef<Location[]>([current()]);
  const pos = useRef(0);
  const navigating = useRef(false);
  const [, refresh] = useState(0);
  useEffect(() => useStore.subscribe(() => {
    if (navigating.current) return;
    const next = current();
    if (same(next, stack.current[pos.current])) return;
    stack.current = [...stack.current.slice(0, pos.current + 1), next].slice(-100);
    pos.current = stack.current.length - 1;
    refresh((n) => n + 1);
  }), []);

  const go = (delta: number) => {
    const target = pos.current + delta;
    if (target < 0 || target >= stack.current.length) return;
    const loc = stack.current[target];
    if (loc.id && !useStore.getState().conversations.some((c) => c.id === loc.id)) {
      stack.current.splice(target, 1);
      if (target < pos.current) pos.current--;
      refresh((n) => n + 1);
      return;
    }
    navigating.current = true;
    pos.current = target;
    const s = useStore.getState();
    if (loc.id !== s.currentId || (!loc.id && loc.group !== s.draftGroupId)) {
      if (loc.id) void s.openChat(loc.id);
      else s.newChat(loc.group);
    }
    useStore.getState().setView(loc.view);
    navigating.current = false;
    refresh((n) => n + 1);
  };
  const windowAction = (action: "minimize" | "toggleMaximize" | "close") => {
    if (isTauri()) void getCurrentWindow()[action]();
  };
  const button = "flex h-8 w-9 items-center justify-center text-muted hover:bg-hover hover:text-fg disabled:opacity-30";
  return (
    <div data-tauri-drag-region className="flex h-9 shrink-0 items-center bg-sidebar select-none">
      <button type="button" aria-label={t("nav.back")} title={t("nav.back")} className={button} disabled={pos.current === 0} onClick={() => go(-1)}><ChevronLeft size={17} /></button>
      <button type="button" aria-label={t("nav.forward")} title={t("nav.forward")} className={button} disabled={pos.current >= stack.current.length - 1} onClick={() => go(1)}><ChevronRight size={17} /></button>
      <div data-tauri-drag-region className="h-full flex-1" />
      <button type="button" aria-label={t("window.minimize")} title={t("window.minimize")} className={button} onClick={() => windowAction("minimize")}><Minus size={15} /></button>
      <button type="button" aria-label={t("window.maximize")} title={t("window.maximize")} className={button} onClick={() => windowAction("toggleMaximize")}><Square size={12} /></button>
      <button type="button" aria-label={t("window.close")} title={t("window.close")} className={`${button} hover:bg-red-600 hover:text-white`} onClick={() => windowAction("close")}><X size={15} /></button>
    </div>
  );
}
