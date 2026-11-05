import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ArrowUp, FileText, Image as ImageIcon, Plus, Square, X } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useSelection, useStore } from "../lib/store";
import { useT } from "../lib/i18n";
import { api } from "../lib/api";
import type { AppError, Attachment } from "../lib/types";
import { ModelPicker } from "./ModelPicker";
import { EffortPicker } from "./EffortPicker";
import { ApprovalChip, ModeChip } from "./ComposerChips";
import { WorkspacePicker } from "./TopBar";

export function Composer({ centered }: { centered: boolean }) {
  const t = useT();
  const sel = useSelection();
  const currentId = useStore((s) => s.currentId);
  const running = useStore((s) => (s.currentId ? !!s.running[s.currentId] : false));
  const providers = useStore((s) => s.providers);
  const send = useStore((s) => s.send);
  const stop = useStore((s) => s.stop);
  const setEffort = useStore((s) => s.setEffort);
  const [text, setText] = useState("");
  const [atts, setAtts] = useState<Attachment[]>([]);
  const [attErr, setAttErr] = useState<string | null>(null);
  const ta = useRef<HTMLTextAreaElement>(null);

  useLayoutEffect(() => {
    const el = ta.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, Math.round(window.innerHeight * 0.35))}px`;
  }, [text]);

  useEffect(() => {
    ta.current?.focus();
  }, [currentId]);

  if (!sel) return null;
  const hasKey = providers.find((p) => p.id === sel.provider)?.has_key;
  const blocked = atts.some((a) => a.kind !== "text");
  const canSend = !running && !!hasKey && (text.trim().length > 0 || atts.length > 0) && !blocked && !!sel.model;

  const submit = () => {
    if (!canSend) return;
    send(text, atts);
    setText("");
    setAtts([]);
  };

  const pickFiles = async () => {
    setAttErr(null);
    const picked = await openDialog({
      multiple: true,
      filters: [{ name: "Files", extensions: ["txt", "md", "markdown", "log", "csv", "png", "jpg", "jpeg", "webp", "pdf", "docx", "xlsx", "pptx"] }],
    });
    const paths = Array.isArray(picked) ? picked : picked ? [picked] : [];
    for (const p of paths) {
      try {
        const a = await api.inspectAttachment(p);
        setAtts((cur) => [...cur, a]);
      } catch (e) {
        setAttErr((e as AppError).message);
      }
    }
  };

  return (
    <div className={`w-full ${centered ? "max-w-[720px]" : "max-w-[800px]"} mx-auto`}>
      {centered && (
        <div className="mb-1.5 flex items-center gap-1 px-1">
          <WorkspacePicker plain />
          <ModeChip />
        </div>
      )}
      <div className="rounded-[22px] border border-line bg-elev shadow-[0_1px_3px_rgba(0,0,0,0.05)] focus-within:border-line-strong">
        {atts.length > 0 && (
          <div className="flex flex-wrap gap-1.5 px-3 pt-3">
            {atts.map((a) => (
              <span
                key={a.id}
                className={`flex max-w-[260px] items-center gap-1.5 rounded-md border px-2 py-1 text-xs ${a.kind === "text" ? "border-line text-fg" : "border-warn/40 text-warn"}`}
                title={a.kind === "text" ? a.name : t("composer.unsupported")}
              >
                {a.kind === "image" ? <ImageIcon size={13} /> : <FileText size={13} />}
                <span className="truncate">{a.name}</span>
                {a.kind !== "text" && <span className="shrink-0">· {t("composer.unsupported")}</span>}
                <button type="button" title={t("composer.removeAttachment")} onClick={() => setAtts((cur) => cur.filter((x) => x.id !== a.id))} className="text-faint hover:text-fg">
                  <X size={12} />
                </button>
              </span>
            ))}
          </div>
        )}
        <textarea
          ref={ta}
          value={text}
          rows={centered ? 3 : 1}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              submit();
            }
          }}
          placeholder={t("composer.placeholder")}
          className="block max-h-[35vh] w-full resize-none bg-transparent px-4 pt-3 pb-1 text-[14px] leading-relaxed outline-none placeholder:text-faint focus-visible:outline-none"
        />
        <div className="flex flex-wrap items-center gap-1 px-2.5 pb-2.5">
          <button
            type="button"
            onClick={pickFiles}
            title={t("composer.attach")}
            className="flex h-8 w-8 items-center justify-center rounded-full bg-hover text-muted hover:bg-line hover:text-fg"
          >
            <Plus size={17} />
          </button>
          <ApprovalChip />
          <div className="ml-auto flex items-center gap-0.5">
            <ModelPicker provider={sel.provider} model={sel.model} caps={sel.caps} />
            <EffortPicker caps={sel.caps} value={sel.effort} onChange={setEffort} />
          </div>
          <div className="ml-1">
            {running ? (
              <button type="button" onClick={stop} title={t("composer.stop")} className="flex h-8 w-8 items-center justify-center rounded-full bg-accent text-accent-fg">
                <Square size={12} fill="currentColor" />
              </button>
            ) : (
              <button
                type="button"
                onClick={submit}
                disabled={!canSend}
                title={t("composer.send")}
                className="flex h-8 w-8 items-center justify-center rounded-full bg-accent text-accent-fg disabled:opacity-25"
              >
                <ArrowUp size={16} />
              </button>
            )}
          </div>
        </div>
      </div>
      <div className="mt-1.5 min-h-[18px] px-2 text-center text-[11px] text-faint">
        {!hasKey ? (
          <span className="text-warn">{t("composer.noKey", { provider: t(`provider.${sel.provider}`) })}</span>
        ) : attErr ? (
          <span className="text-danger">{attErr}</span>
        ) : (
          !centered && t("composer.hint")
        )}
      </div>
    </div>
  );
}
