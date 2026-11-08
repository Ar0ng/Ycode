import { useState } from "react";
import { Ban, Check, ChevronRight, CircleDashed, Loader2, X } from "lucide-react";
import { toolStateKey, useStore } from "../lib/store";
import { useT } from "../lib/i18n";
import type { ToolCall, ToolResult, ToolStatus } from "../lib/types";
import { GeneratedImages } from "./GeneratedImages";

function summary(call: ToolCall): string {
  const a = call.arguments as Record<string, unknown>;
  const v = a.command ?? a.path ?? a.pattern ?? a.prompt ?? a.name ?? "";
  return typeof v === "string" ? v : JSON.stringify(v);
}

export function Diff({ text }: { text: string }) {
  return (
    <pre className="max-h-80 overflow-auto rounded-md border border-line bg-code py-1.5 font-mono text-[12px] leading-[1.55]">
      {text.split("\n").map((l, i) => (
        <div key={i} className={`px-2.5 whitespace-pre ${l.startsWith("+") && !l.startsWith("+++") ? "diff-add" : l.startsWith("-") && !l.startsWith("---") ? "diff-del" : l.startsWith("@@") ? "text-faint" : ""}`}>
          {l || " "}
        </div>
      ))}
    </pre>
  );
}

const ICON: Record<ToolStatus, React.ReactNode> = {
  pending: <CircleDashed size={14} className="text-faint" />,
  running: <Loader2 size={14} className="animate-spin text-muted" />,
  success: <Check size={14} className="text-ok" />,
  failed: <X size={14} className="text-danger" />,
  cancelled: <Ban size={14} className="text-faint" />,
};

export function ToolCard({ call, result, compact = false, expanded = false }: { call: ToolCall; result?: ToolResult; compact?: boolean; expanded?: boolean }) {
  const t = useT();
  const live = useStore((s) => s.currentId ? s.tools[toolStateKey(s.currentId, call.id)] : undefined);
  const [open, setOpen] = useState(false);
  const running = useStore((s) => (s.currentId ? !!s.running[s.currentId] : false));
  // No saved result and nothing running: the call was interrupted (app closed /
  // restarted mid-tool); show it as cancelled instead of "waiting" forever.
  const status: ToolStatus = (result?.status as ToolStatus) ?? live?.status ?? (running ? "pending" : "cancelled");
  const approval = !result ? live?.approval : undefined;
  const diff = result?.diff;
  const important = ["write_file", "edit_file", "run_command", "inspect_visual", "generate_image", "edit_image"].includes(call.name);
  if (approval) return null; // The persistent approval panel remains visible above the composer.
  if (compact && !expanded && !approval && status !== "failed" && !(result?.images?.length) && !(status === "running" && important)) return null;

  return (
    <div className="msg my-1.5 rounded-lg border border-line text-[13px]">
      <button type="button" onClick={() => setOpen((o) => !o)} className="flex w-full items-center gap-2 px-3 py-1.5 text-left">
        {ICON[status]}
        <span className="font-mono text-xs text-muted">{call.name}</span>
        <span className="min-w-0 flex-1 truncate font-mono text-xs text-fg">{summary(call)}</span>
        <span className="text-[11px] text-faint">{status === "running" && (call.name === "generate_image" || call.name === "edit_image") ? t("chat.generatingImage") : t(`tool.${status}`)}</span>
        <ChevronRight size={14} className={`text-faint transition-transform ${open ? "rotate-90" : ""}`} />
      </button>
      {result?.images && result.images.length > 0 && <GeneratedImages ids={result.images} />}
      {open && (
        <div className="space-y-2 border-t border-line px-3 py-2">
          <pre className="overflow-x-auto font-mono text-[11.5px] whitespace-pre-wrap text-muted">{JSON.stringify(call.arguments, null, 2)}</pre>
          {diff && (
            <div>
              <div className="mb-1 text-[11px] text-faint">{t("tool.showDiff")}</div>
              <Diff text={diff} />
            </div>
          )}
          {result && (
            <div>
              <div className="mb-1 text-[11px] text-faint">{t("tool.showOutput")}</div>
              <pre className="max-h-72 overflow-auto rounded-md border border-line bg-code px-2.5 py-1.5 font-mono text-[11.5px] whitespace-pre-wrap">{result.content}</pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
