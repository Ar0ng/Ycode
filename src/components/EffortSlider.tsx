import { useRef } from "react";
import { Info } from "lucide-react";
import { useT } from "../lib/i18n";
import { supportedEfforts } from "../lib/effort";
import type { Effort, ModelCaps } from "../lib/types";

/**
 * Discrete reasoning-effort slider. Only the levels the selected model really
 * accepts are rendered; the value shown is the one that will be sent.
 */
export function EffortSlider({ caps, value, onChange }: { caps?: ModelCaps; value: Effort | null; onChange: (e: Effort) => void }) {
  const t = useT();
  const track = useRef<HTMLDivElement>(null);
  const levels = supportedEfforts(caps);
  const disabled = levels.length < 2 || !value;

  if (disabled) {
    const why = !caps || caps.source === "unknown" || caps.source === "manual" ? t("effort.disabledUnknown") : t("effort.disabled", { why: caps.note ?? "—" });
    return (
      <div className="flex items-center gap-1.5 text-xs text-faint" title={why}>
        <span>{t("effort.label")}</span>
        <span className="rounded border border-line px-1.5 py-0.5">{value ? t(`effort.${value}`) : "—"}</span>
        <Info size={12} />
      </div>
    );
  }

  const idx = Math.max(0, levels.indexOf(value));
  const pick = (clientX: number) => {
    const el = track.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const ratio = Math.min(1, Math.max(0, (clientX - r.left) / r.width));
    const i = Math.round(ratio * (levels.length - 1));
    if (levels[i] !== value) onChange(levels[i]);
  };
  const hint = caps?.effort_update === "param" ? t("effort.param") : t("effort.inBand");

  return (
    <div className="flex items-center gap-2 text-xs" title={`${t("effort.active", { level: t(`effort.${value}`) })}\n${hint}`}>
      <span className="text-muted">{t("effort.label")}</span>
      <div
        ref={track}
        role="slider"
        tabIndex={0}
        aria-label={t("effort.label")}
        aria-valuemin={0}
        aria-valuemax={levels.length - 1}
        aria-valuenow={idx}
        aria-valuetext={t(`effort.${value}`)}
        className="relative h-5 w-24 cursor-pointer touch-none select-none"
        onPointerDown={(e) => {
          (e.target as HTMLElement).setPointerCapture?.(e.pointerId);
          pick(e.clientX);
        }}
        onPointerMove={(e) => {
          if (e.buttons === 1) pick(e.clientX);
        }}
        onKeyDown={(e) => {
          if (e.key === "ArrowRight" || e.key === "ArrowUp") {
            e.preventDefault();
            onChange(levels[Math.min(levels.length - 1, idx + 1)]);
          } else if (e.key === "ArrowLeft" || e.key === "ArrowDown") {
            e.preventDefault();
            onChange(levels[Math.max(0, idx - 1)]);
          }
        }}
      >
        <div className="absolute top-1/2 right-0 left-0 h-[3px] -translate-y-1/2 rounded-full bg-line-strong" />
        <div className="absolute top-1/2 left-0 h-[3px] -translate-y-1/2 rounded-full bg-fg/70" style={{ width: `${(idx / (levels.length - 1)) * 100}%` }} />
        {levels.map((l, i) => (
          <span
            key={l}
            title={t(`effort.${l}`)}
            className={`absolute top-1/2 h-1.5 w-1.5 -translate-x-1/2 -translate-y-1/2 rounded-full ${i <= idx ? "bg-fg/70" : "bg-line-strong"}`}
            style={{ left: `${(i / (levels.length - 1)) * 100}%` }}
          />
        ))}
        <span
          className="absolute top-1/2 h-3.5 w-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full border border-line-strong bg-elev shadow-sm transition-[left] duration-100"
          style={{ left: `${(idx / (levels.length - 1)) * 100}%` }}
        />
      </div>
      <span className="min-w-[4.5em] font-medium text-fg">{t(`effort.${value}`)}</span>
    </div>
  );
}
