import { useMemo, useState } from "react";
import { AlertTriangle, Loader2, ShieldQuestion } from "lucide-react";
import { useStore } from "../lib/store";
import { useT } from "../lib/i18n";
import { Diff } from "./ToolCard";
import type { ApprovalPrompt } from "../lib/types";

/** The approval the running reply in the open chat is waiting for, if any. */
export function usePendingApproval(): ApprovalPrompt | undefined {
  const currentId = useStore((s) => s.currentId);
  const running = useStore((s) => (currentId ? !!s.running[currentId] : false));
  const tools = useStore((s) => s.tools);
  return useMemo(
    () => (running ? Object.values(tools).find((v) => v.conversationId === currentId && v.approval)?.approval : undefined),
    [tools, currentId, running],
  );
}

/** One status line above the input box: the running tool, or waiting for the provider. */
export function ActiveTool() {
  const t = useT();
  const currentId = useStore((s) => s.currentId);
  const running = useStore((s) => (currentId ? !!s.running[currentId] : false));
  const tools = useStore((s) => s.tools);
  const waiting = useStore((s) => (currentId ? !!s.providerWaiting[currentId] : false));
  const active = useMemo(() => Object.values(tools).filter((v) => v.conversationId === currentId && v.status === "running" && !v.approval), [tools, currentId]);
  if (!running) return null;
  const line = (text: string) => (
    <div role="status" className="mx-auto mb-2 flex max-w-[800px] items-center gap-1.5 px-2 text-xs text-muted">
      <Loader2 size={13} className="animate-spin" />
      {text}
    </div>
  );
  if (active.length) return line(t("tool.runningNow", { name: active[active.length - 1].name }));
  return waiting ? line(t("tool.waitingProvider")) : null;
}

/** Takes the input box's place while a tool call waits for the user's decision. */
export function ApprovalPanel({ approval }: { approval: ApprovalPrompt }) {
  const approve = useStore((s) => s.approve);
  const error = useStore((s) => s.lastError);
  const [busy, setBusy] = useState(false);
  const decide = async (decision: "allow" | "allow_session" | "deny") => {
    setBusy(true);
    try {
      await approve(approval.request_id, decision);
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="mx-auto w-full max-w-[800px]">
      <ApprovalDetails key={approval.request_id} approval={approval} busy={busy} error={error?.message} decide={decide} />
    </div>
  );
}

/**
 * Header, warning and the Allow / Deny buttons stay fixed; only the command
 * and diff in the middle scroll, so the decision is always one click away.
 */
export function ApprovalDetails({ approval, busy, error, decide }: { approval: ApprovalPrompt; busy: boolean; error?: string; decide: (decision: "allow" | "allow_session" | "deny") => Promise<void> }) {
  const t = useT();
  return (
    <section role="alert" className="fade-in flex max-h-[45vh] flex-col overflow-hidden rounded-[22px] border border-line-strong bg-elev text-[13px] shadow-[0_1px_3px_rgba(0,0,0,0.05)]">
      <div className="flex shrink-0 items-center gap-1.5 px-4 pt-3 pb-2 font-medium">
        <ShieldQuestion size={15} className="text-muted" />
        {t(`tool.${approval.kind}`)}
      </div>
      <div className="min-h-0 flex-1 space-y-2 overflow-y-auto px-4">
        <pre className="overflow-x-auto rounded-md border border-line bg-code px-2.5 py-1.5 font-mono text-[12px] whitespace-pre-wrap break-all">{approval.target}</pre>
        {approval.diff && <Diff text={approval.diff} />}
      </div>
      <div className="shrink-0 px-4 pt-2 pb-3">
        {approval.warning && (
          <div className="mb-1.5 flex items-start gap-1.5 text-xs text-warn">
            <AlertTriangle size={13} className="mt-px shrink-0" />
            {t("tool.warning", { why: approval.warning })}
          </div>
        )}
        {error && <div className="mb-1.5 text-xs text-danger">{error}</div>}
        <div className="flex flex-wrap items-center gap-1.5">
          <button type="button" autoFocus disabled={busy} onClick={() => void decide("allow")} className="rounded-full bg-accent px-4 py-1.5 text-xs font-medium text-accent-fg disabled:opacity-50">
            {t("tool.approve")}
          </button>
          <button type="button" disabled={busy} onClick={() => void decide("allow_session")} className="rounded-full border border-line px-4 py-1.5 text-xs hover:bg-hover disabled:opacity-50">
            {t("tool.approveSession")}
          </button>
          <button type="button" disabled={busy} onClick={() => void decide("deny")} className="ml-auto rounded-full border border-line px-4 py-1.5 text-xs text-danger hover:bg-hover disabled:opacity-50">
            {t("tool.deny")}
          </button>
        </div>
      </div>
    </section>
  );
}
