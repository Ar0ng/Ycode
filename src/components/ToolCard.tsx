import { useState } from "react";
import { AlertTriangle, Ban, Check, ChevronRight, CircleDashed, Loader2, X } from "lucide-react";
import { useStore } from "../lib/store";
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

export function ToolCard({ call, result }: { call: ToolCall; result?: ToolResult }) {
  const t = useT();
  const live = useStore((s) => s.tools[call.id]);
  const approve = useStore((s) => s.approve);
  const [open, setOpen] = useState(false);
  const status: ToolStatus = (result?.status as ToolStatus) ?? live?.status ?? "pending";
  const approval = !result ? live?.approval : undefined;
  const diff = result?.diff ?? approval?.diff;

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
      {approval && (
        <div className="border-t border-line px-3 py-2.5">
          <div className="mb-1.5 font-medium">{t(`tool.${approval.kind}`)}</div>
          {approval.kind === "run_command" && <pre className="mb-2 overflow-x-auto rounded-md border border-line bg-code px-2.5 py-1.5 font-mono text-[12px] whitespace-pre-wrap">{approval.target}</pre>}
          {approval.kind !== "run_command" && <div className="mb-2 font-mono text-xs break-all text-muted">{approval.target}</div>}
          {approval.warning && (
            <div className="mb-2 flex items-center gap-1.5 text-xs text-warn">
              <AlertTriangle size={13} />
              {t("tool.warning", { why: approval.warning })}
            </div>
          )}
          {approval.diff && <Diff text={approval.diff} />}
          <div className="mt-2 flex flex-wrap gap-1.5">
            <button type="button" onClick={() => approve(approval.request_id, "allow")} className="rounded-md bg-accent px-3 py-1 text-xs font-medium text-accent-fg">
              {t("tool.approve")}
            </button>
            <button type="button" onClick={() => approve(approval.request_id, "allow_session")} className="rounded-md border border-line px-3 py-1 text-xs hover:bg-hover">
              {t("tool.approveSession")}
            </button>
            <button type="button" onClick={() => approve(approval.request_id, "deny")} className="rounded-md border border-line px-3 py-1 text-xs text-danger hover:bg-hover">
              {t("tool.deny")}
            </button>
          </div>
        </div>
      )}
      {open && (
        <div className="space-y-2 border-t border-line px-3 py-2">
          <pre className="overflow-x-auto font-mono text-[11.5px] whitespace-pre-wrap text-muted">{JSON.stringify(call.arguments, null, 2)}</pre>
          {diff && !approval && (
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
