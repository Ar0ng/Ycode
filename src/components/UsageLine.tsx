import { useT } from "../lib/i18n";
import type { Usage } from "../lib/types";

export function fmt(n: number) {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(n >= 10_000_000 ? 0 : 1)}M`;
  if (n >= 1000) return `${(n / 1000).toFixed(n >= 10_000 ? 0 : 1)}k`;
  return String(n);
}

/** Cache hit rate = cached / eligible, per provider definition; N/A without a reliable denominator. */
export function hitRate(u: Usage): number | null {
  const d = u.eligible_input_tokens;
  if (d == null || d <= 0) return null;
  return u.cached_input_tokens / d;
}

export function UsageLine({ usage, compact = false, label }: { usage: Usage; compact?: boolean; label?: string }) {
  const t = useT();
  const rate = hitRate(usage);
  const parts = [
    `${t("usage.in")} ${fmt(usage.input_tokens)}`,
    `${t("usage.out")} ${fmt(usage.output_tokens)}`,
    `${t("usage.cached")} ${fmt(usage.cached_input_tokens)}`,
    `${t("usage.hit")} ${rate == null ? t("usage.na") : `${Math.round(rate * 100)}%`}`,
  ];
  if (!compact && usage.cache_write_tokens) parts.push(`${t("usage.write")} ${fmt(usage.cache_write_tokens)}`);
  return (
    <span className="tabular-nums" title={label}>
      {label && <span className="mr-1">{label}:</span>}
      {parts.join(" · ")}
    </span>
  );
}
