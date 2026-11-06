import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ArrowDown, ChevronRight, FileText, RotateCw } from "lucide-react";
import { useStore } from "../lib/store";
import { formatDuration, useNow } from "../lib/useNow";
import { useT } from "../lib/i18n";
import type { AppError, Message, ToolResult } from "../lib/types";
import { CopyButton, Markdown } from "./Markdown";
import { ToolCard } from "./ToolCard";
import { UsageLine } from "./UsageLine";

const PAGE = 60;

function Reasoning({ text, live, seconds }: { text: string; live: boolean; seconds: string | null }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <div className="mb-1.5">
      <button type="button" onClick={() => setOpen((o) => !o)} className="flex items-center gap-1 text-xs text-faint hover:text-muted">
        <ChevronRight size={13} className={`transition-transform ${open ? "rotate-90" : ""}`} />
        {live ? t("chat.thinkingLive", { s: seconds ?? "" }) : seconds ? t("chat.thoughtFor", { s: seconds }) : t("chat.reasoning")}
        {live && <span className="pulse-dot ml-1 inline-block h-1 w-1 rounded-full bg-faint" />}
      </button>
      {open && <div className="mt-1 border-l-2 border-line pl-3 text-[13px] whitespace-pre-wrap text-muted">{text}</div>}
    </div>
  );
}

export function ErrorBox({ error, onRetry }: { error: AppError; onRetry?: () => void }) {
  const t = useT();
  const [details, setDetails] = useState(false);
  const kind = error.provider?.kind;
  const msg = kind ? t(`err.${kind}`) : error.message;
  return (
    <div className="msg my-2 rounded-lg border border-danger/30 px-3 py-2 text-[13px]">
      <div className="flex items-center gap-2">
        <span className="text-danger">{msg}</span>
        <span className="flex-1" />
        {kind && (
          <button type="button" onClick={() => setDetails((d) => !d)} className="text-xs text-faint hover:text-muted">
            {t("err.details")}
          </button>
        )}
        {onRetry && (
          <button type="button" onClick={onRetry} className="flex items-center gap-1 rounded-md border border-line px-2 py-0.5 text-xs hover:bg-hover">
            <RotateCw size={12} />
            {t("chat.retry")}
          </button>
        )}
      </div>
      {details && error.provider && (
        <div className="mt-1 font-mono text-[11px] break-all text-muted">
          {error.provider.status ? `HTTP ${error.provider.status} · ` : ""}
          {error.provider.detail}
        </div>
      )}
    </div>
  );
}

const UserBubble = memo(function UserBubble({ m }: { m: Message }) {
  return (
    <div className="msg fade-in flex flex-col items-end py-2">
      <div className="max-w-[85%] rounded-2xl bg-hover px-4 py-2.5">
        {m.attachments && m.attachments.length > 0 && (
          <div className="mb-1.5 flex flex-wrap gap-1">
            {m.attachments.map((a) => (
              <span key={a.id} className="flex items-center gap-1 rounded border border-line px-1.5 py-0.5 text-[11px] text-muted">
                <FileText size={11} />
                {a.name}
              </span>
            ))}
          </div>
        )}
        <div className="text-[14px] leading-relaxed whitespace-pre-wrap">{m.text}</div>
      </div>
      {m.text && <CopyButton text={m.text} className="mt-0.5 text-faint" />}
    </div>
  );
});

function AssistantBlock({ m, results, live }: { m: Message; results: Map<string, ToolResult>; live: boolean }) {
  const t = useT();
  const streaming = m.status === "streaming";
  const thinkingNow = streaming && !m.text;
  const now = useNow(thinkingNow);
  // While streaming: live seconds since the request started, frozen when the
  // first text arrives; afterwards the backend's exact measurement.
  const frozen = useRef<number | null>(null);
  if (streaming && m.text && frozen.current === null) frozen.current = Date.now() - m.created_at;
  const thinkingMs = m.timing?.thinking_ms ?? (streaming ? (frozen.current ?? now - m.created_at) : null);
  const seconds = thinkingMs != null ? formatDuration(thinkingMs) : null;
  return (
    <div className="msg py-2">
      {m.reasoning && <Reasoning text={m.reasoning} live={thinkingNow} seconds={seconds} />}
      {thinkingNow && !m.reasoning && !(m.tool_calls?.length) && (
        <div className="flex items-center gap-1.5 text-xs text-faint">
          <span className="pulse-dot inline-block h-1.5 w-1.5 rounded-full bg-faint" />
          {t("chat.thinkingLive", { s: seconds ?? "" })}
        </div>
      )}
      {m.text && <Markdown text={m.text} />}
      {m.tool_calls?.map((c) => <ToolCard key={c.id} call={c} result={results.get(c.id)} />)}
      {!streaming && (m.text || m.status === "partial") && (
        <div className="mt-1 flex items-center gap-2 text-[11px] text-faint">
          {m.status === "partial" && <span>{t("chat.stopped")}</span>}
          {m.text && <CopyButton text={m.text} className="-ml-1.5" />}
          {m.timing && !live && <span className="tabular-nums">{t("stats.thinking", { s: formatDuration(m.timing.thinking_ms) })}</span>}
          {m.usage && !live && <UsageLine usage={m.usage} compact />}
        </div>
      )}
    </div>
  );
}

export function MessageList() {
  const t = useT();
  const messages = useStore((s) => s.messages);
  const running = useStore((s) => (s.currentId ? !!s.running[s.currentId] : false));
  const lastError = useStore((s) => s.lastError);
  const retry = useStore((s) => s.retry);
  const scroller = useRef<HTMLDivElement>(null);
  const [atBottom, setAtBottom] = useState(true);
  const [limit, setLimit] = useState(PAGE);
  const prevLen = useRef(0);

  const results = useMemo(() => {
    const map = new Map<string, ToolResult>();
    for (const m of messages) for (const r of m.tool_results ?? []) map.set(r.call_id, r);
    return map;
  }, [messages]);

  const visible = messages.filter((m) => m.role !== "tool" && !(m.role === "assistant" && m.status === "error" && !m.text && !m.reasoning));
  const hidden = Math.max(0, visible.length - limit);
  const shown = visible.slice(hidden);

  // Follow the stream only while the user is at the bottom; jump down when they send.
  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const userSent = messages.length > prevLen.current && messages[messages.length - 1]?.role === "user";
    prevLen.current = messages.length;
    if (atBottom || userSent) el.scrollTop = el.scrollHeight;
  }, [messages, atBottom]);

  useEffect(() => setLimit(PAGE), [messages.length === 0]);

  return (
    <div className="relative min-h-0 flex-1">
      <div
        ref={scroller}
        className="h-full overflow-y-auto"
        onScroll={(e) => {
          const el = e.currentTarget;
          setAtBottom(el.scrollHeight - el.scrollTop - el.clientHeight < 48);
        }}
      >
        <div className="mx-auto w-full max-w-[800px] px-6 pt-6 pb-4">
          {hidden > 0 && (
            <button type="button" onClick={() => setLimit((l) => l + PAGE)} className="mx-auto mb-3 block rounded-md px-3 py-1 text-xs text-muted hover:bg-hover">
              {t("chat.showEarlier", { n: hidden })}
            </button>
          )}
          {shown.map((m, i) => {
            if (m.role === "user") return <UserBubble key={m.id} m={m} />;
            if (m.role === "control")
              return m.control ? (
                <div key={m.id} className="msg my-2 text-center text-[11px] text-faint">
                  {t("chat.effortChanged", { level: t(`effort.${m.control.effort}`) })}
                </div>
              ) : null;
            return <AssistantBlock key={m.id} m={m} results={results} live={running && i === shown.length - 1} />;
          })}
          {lastError && !running && <ErrorBox error={lastError} onRetry={lastError.code === "provider" ? retry : undefined} />}
        </div>
      </div>
      {!atBottom && (
        <button
          type="button"
          title={t("chat.scrollDown")}
          onClick={() => scroller.current?.scrollTo({ top: scroller.current.scrollHeight, behavior: "smooth" })}
          className="absolute bottom-3 left-1/2 flex h-8 w-8 -translate-x-1/2 items-center justify-center rounded-full border border-line bg-elev text-muted shadow-sm hover:text-fg"
        >
          <ArrowDown size={15} />
        </button>
      )}
    </div>
  );
}
