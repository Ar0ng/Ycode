import { useMemo } from "react";
import { BarChart3, ChevronDown } from "lucide-react";
import { useSelection, useStore } from "../lib/store";
import { useT } from "../lib/i18n";
import type { Usage } from "../lib/types";
import { Menu, useDropdown } from "./Popover";
import { fmt, hitRate } from "./UsageLine";

export function contextEstimate(usage: Usage | null, window: number | undefined) {
  if (!usage || !window || window <= 0) return null;
  const used = usage.input_tokens + usage.output_tokens;
  return { used, remaining: Math.max(0, window - used), window };
}

export function UsageSummary() {
  const t = useT();
  const selection = useSelection();
  const messages = useStore((s) => s.messages);
  const total = useStore((s) => s.convUsage);
  const lastMessage = useMemo(() => [...messages].reverse().find((m) => m.role === "assistant" && m.usage), [messages]);
  const last = lastMessage?.usage ?? null;
  const matching = lastMessage?.provider === selection?.provider && lastMessage?.model === selection?.model;
  const window = selection?.caps && !["unknown", "manual"].includes(selection.caps.source) ? selection.caps.context_window : undefined;
  const context = contextEstimate(matching ? last : null, window);
  const rate = last ? hitRate(last) : null;
  const deepseek = useMemo(() => messages.filter((m) => m.role === "assistant" && m.provider === "deepseek" && m.usage).slice(-5).reverse(), [messages]);
  const { open, setOpen, ref } = useDropdown();
  const row = (label: string, value: string) => <div className="flex justify-between gap-6"><span className="text-muted">{label}</span><span className="tabular-nums text-fg">{value}</span></div>;
  const n = (value: number | null | undefined) => value == null ? t("usage.na") : fmt(value);
  return (
    <div ref={ref} className="relative mx-auto mt-1.5 flex max-w-[800px] justify-center">
      <button type="button" aria-expanded={open} aria-label={t("usage.details")} onClick={() => setOpen((v) => !v)} className="flex flex-wrap items-center justify-center gap-x-2 rounded-md px-2 py-1 text-[11px] text-faint hover:bg-hover hover:text-muted">
        <BarChart3 size={12} />
        <span>{t("usage.context")} {context ? `≈${fmt(context.remaining)}` : t("usage.na")}</span>
        <span>· {t("usage.out")} {n(last?.output_tokens)}</span>
        <span>· {t("usage.hit")} {rate == null ? t("usage.na") : `${Math.round(rate * 100)}%`}</span>
        <ChevronDown size={12} />
      </button>
      {open && (
        <Menu up className="left-1/2 max-h-[70vh] w-72 -translate-x-1/2 overflow-y-auto p-3 text-xs">
          <div className="mb-2 font-medium text-fg">{t("usage.details")}</div>
          <div className="space-y-1.5">
            {row(t("usage.context"), context ? `≈${n(context.remaining)}` : t("usage.na"))}
            {row(t("usage.used"), context ? `≈${n(context.used)}` : t("usage.na"))}
            {row(t("usage.window"), n(window))}
          </div>
          <p className="mt-2 leading-relaxed text-faint">{t("usage.contextHint")}</p>
          <div className="mt-2 border-t border-line pt-2">
            <div className="mb-1.5 font-medium text-fg">{t("usage.last")}</div>
            {last ? <div className="space-y-1.5">{row(t("usage.in"), n(last.input_tokens))}{row(t("usage.out"), n(last.output_tokens))}{row(t("usage.reasoning"), n(last.reasoning_tokens))}{row(t("usage.cached"), n(last.cached_input_tokens))}{row(t("usage.write"), n(last.cache_write_tokens))}{row(t("usage.hit"), rate == null ? t("usage.na") : `${Math.round(rate * 100)}%`)}</div> : <p className="text-faint">{t("usage.noData")}</p>}
          </div>
          {total && <div className="mt-2 space-y-1.5 border-t border-line pt-2"><div className="font-medium text-fg">{t("usage.total")}</div>{row(t("usage.in"), n(total.input_tokens))}{row(t("usage.out"), n(total.output_tokens))}{row(t("usage.cached"), n(total.cached_input_tokens))}</div>}
          {deepseek.length > 0 && <div className="mt-2 space-y-1.5 border-t border-line pt-2"><div className="font-medium text-fg">{t("usage.cacheHistory")}</div>{deepseek.map((m) => { const u = m.usage!; const hit = hitRate(u); const miss = u.eligible_input_tokens == null ? null : Math.max(0, u.eligible_input_tokens - u.cached_input_tokens); return <div key={m.id} className="text-faint">{m.model} · {t("usage.cached")} {n(u.cached_input_tokens)} · {t("usage.miss")} {n(miss)} · {hit == null ? t("usage.na") : `${(hit * 100).toFixed(1)}%`}</div>; })}<p className="leading-relaxed text-faint">{t("usage.cacheHint")}</p></div>}
        </Menu>
      )}
    </div>
  );
}
