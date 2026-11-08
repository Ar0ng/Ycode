import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ArrowDown, ChevronRight, FileText, RotateCw } from "lucide-react";
import { useStore } from "../lib/store";
import { formatDuration, useNow } from "../lib/useNow";
import { useT } from "../lib/i18n";
import type { AppError, Message, ToolResult } from "../lib/types";
import { CopyButton, Markdown } from "./Markdown";
import { ToolCard } from "./ToolCard";
import { UsageLine } from "./UsageLine";
import { displayTurns, type DisplayTurn } from "../lib/turns";

const PAGE = 60;

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

function TurnBlock({ turn, results, live }: { turn: DisplayTurn; results: Map<string, ToolResult>; live: boolean }) {
  const t = useT();
  const minimal = useStore((s) => (s.settings?.activity_display ?? "minimal") === "minimal");
  const [open, setOpen] = useState(false);
  const now = useNow(live, 1000);
  const assistants = turn.assistants;
  const last = assistants[assistants.length - 1];
  const final = last && !last.tool_calls?.length && last.text ? last : undefined;
  const started = assistants.find((m) => m.run_started_at != null)?.run_started_at ?? turn.user?.run_started_at;
  const duration = [...assistants].reverse().find((m) => m.run_duration_ms != null)?.run_duration_ms;
  const seconds = live && started != null ? formatDuration(now - started) : duration != null ? formatDuration(duration) : null;
  const hasActivity = live || assistants.some((m) => m.reasoning || m.tool_calls?.length || m.timing || m.run_duration_ms != null);
  return (
    <>
      {turn.user && <UserBubble m={turn.user} />}
      {hasActivity && <button type="button" aria-expanded={open} onClick={() => setOpen((v) => !v)} className="msg mt-2 mb-1 flex w-full items-center gap-1 border-b border-line pb-2 text-left text-xs text-faint hover:text-muted">
        {live ? t("chat.thinkingLive", { s: seconds ?? "" }) : seconds ? t("chat.elapsed", { s: seconds }) : t("chat.reasoning")}
        <ChevronRight size={13} className={`transition-transform ${open ? "rotate-90" : ""}`} />
        {live && <span className="pulse-dot ml-1 inline-block h-1 w-1 rounded-full bg-faint" />}
      </button>}
      {assistants.map((m) => <div key={m.id}>
        {open && m.reasoning && <div className="mt-1 border-l-2 border-line pl-3 text-[13px] whitespace-pre-wrap text-muted">{m.reasoning}</div>}
        {(open || !minimal) && m !== final && m.text && <div className="msg py-1"><Markdown text={m.text} /></div>}
        {m.tool_calls?.map((c) => <ToolCard key={c.id} call={c} result={results.get(c.id)} compact={minimal} expanded={open} />)}
      </div>)}
      {turn.guidance.map((m) => <UserBubble key={m.id} m={m} />)}
      {final && <div className="msg py-2"><Markdown text={final.text} /></div>}
      {last && last.status !== "streaming" && (final || last.status === "partial") && (
        <div className="mt-1 flex items-center gap-2 text-[11px] text-faint">
          {last.status === "partial" && <span>{t("chat.stopped")}</span>}
          {final && <CopyButton text={final.text} className="-ml-1.5" />}
          {last.usage && !live && <UsageLine usage={last.usage} compact />}
        </div>
      )}
    </>
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

  const visible = useMemo(() => displayTurns(messages), [messages]);
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
          {shown.map((item, i) => {
            if (item.kind === "control")
              return item.message.control ? (
                <div key={item.message.id} className="msg my-2 text-center text-[11px] text-faint">
                  {t("chat.effortChanged", { level: t(`effort.${item.message.control.effort}`) })}
                </div>
              ) : null;
            return <TurnBlock key={item.turn.key} turn={item.turn} results={results} live={running && i === shown.length - 1} />;
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
