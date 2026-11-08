import { useMemo, useState } from "react";
import { AlertTriangle, Loader2 } from "lucide-react";
import { useStore } from "../lib/store";
import { useT } from "../lib/i18n";
import { Diff } from "./ToolCard";
import type { ApprovalPrompt } from "../lib/types";

export function ActiveTool() {
  const t = useT();
  const currentId = useStore((s) => s.currentId);
  const running = useStore((s) => currentId ? !!s.running[currentId] : false);
  const tools = useStore((s) => s.tools);
  const waiting = useStore((s) => currentId ? !!s.providerWaiting[currentId] : false);
  const error = useStore((s) => s.lastError);
  const approve = useStore((s) => s.approve);
  const [busy, setBusy] = useState(false);
  const active = useMemo(() => Object.values(tools).filter((v) => v.conversationId === currentId && (v.approval || v.status === "running")), [tools, currentId]);
  if (!running) return null;
  if (!active.length) return waiting ? <div role="status" className="mx-auto mb-2 flex max-w-[800px] items-center gap-1.5 px-2 text-xs text-muted"><Loader2 size={13} className="animate-spin" />{t("tool.waitingProvider")}</div> : null;
  const item = active.find((v) => v.approval) ?? active[active.length - 1];
  const approval = item.approval;
  if (!approval) return <div role="status" className="mx-auto mb-2 flex max-w-[800px] items-center gap-1.5 px-2 text-xs text-muted"><Loader2 size={13} className="animate-spin" />{t("tool.runningNow", { name: item.name })}</div>;
  const decide = async (decision: "allow" | "allow_session" | "deny") => {
    setBusy(true);
    try { await approve(approval.request_id, decision); } finally { setBusy(false); }
  };
  return <ApprovalDetails approval={approval} busy={busy} error={error?.message} decide={decide} />;
}

export function ApprovalDetails({ approval, busy, error, decide }: { approval: ApprovalPrompt; busy: boolean; error?: string; decide: (decision: "allow" | "allow_session" | "deny") => Promise<void> }) {
  const t = useT();
  return (
    <section role="alert" className="mx-auto mb-2 max-h-[35vh] max-w-[800px] overflow-y-auto rounded-xl border border-line-strong bg-elev px-3 py-2.5 text-[13px] shadow-sm">
      <div className="mb-1.5 font-medium">{t(`tool.${approval.kind}`)}</div>
      <pre className="mb-2 overflow-x-auto rounded-md border border-line bg-code px-2.5 py-1.5 font-mono text-[12px] whitespace-pre-wrap break-all">{approval.target}</pre>
      {approval.warning && <div className="mb-2 flex items-center gap-1.5 text-xs text-warn"><AlertTriangle size={13} />{t("tool.warning", { why: approval.warning })}</div>}
      {approval.diff && <Diff text={approval.diff} />}
      {error && <div className="mt-2 text-xs text-danger">{error}</div>}
      <div className="mt-2 flex flex-wrap gap-1.5">
        <button type="button" disabled={busy} onClick={() => void decide("allow")} className="rounded-md bg-accent px-3 py-1 text-xs font-medium text-accent-fg disabled:opacity-50">{t("tool.approve")}</button>
        <button type="button" disabled={busy} onClick={() => void decide("allow_session")} className="rounded-md border border-line px-3 py-1 text-xs hover:bg-hover disabled:opacity-50">{t("tool.approveSession")}</button>
        <button type="button" disabled={busy} onClick={() => void decide("deny")} className="rounded-md border border-line px-3 py-1 text-xs text-danger hover:bg-hover disabled:opacity-50">{t("tool.deny")}</button>
      </div>
    </section>
  );
}
