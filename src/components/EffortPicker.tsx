import { Brain, Check, ChevronDown } from "lucide-react";
import { useT, type Key } from "../lib/i18n";
import { supportedEfforts } from "../lib/effort";
import { EFFORT_PARAM, effortWire } from "../lib/models";
import type { Effort, ModelCaps } from "../lib/types";
import { Menu, useDropdown } from "./Popover";

const NOTE: Record<string, Key> = { openai: "effort.openaiNote", deepseek: "effort.deepseekNote", anthropic: "effort.anthropicNote" };

/**
 * Reasoning-effort dropdown (same style as the model picker). It lists exactly
 * the levels the selected model accepts, marks the model's default, and shows
 * the provider-specific value that will be sent.
 */
export function EffortPicker({ caps, value, onChange }: { caps?: ModelCaps; value: Effort | null; onChange: (e: Effort) => void }) {
  const t = useT();
  const { open, setOpen, ref } = useDropdown();
  const levels = supportedEfforts(caps);
  const disabled = !caps || levels.length < 2 || !value;

  if (disabled) {
    const why = !caps || caps.source === "unknown" || caps.source === "manual" ? t("effort.disabledUnknown") : t("effort.disabled", { why: caps.note ?? "—" });
    return (
      <span className="flex items-center gap-1 rounded-md px-2 py-1 text-xs text-faint" title={why}>
        <Brain size={13} />
        {value ? t(`effort.${value}`) : "—"}
      </span>
    );
  }

  const hint = caps.effort_update === "param" ? t("effort.param") : t("effort.inBand");
  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        title={`${t("effort.label")} · ${t("effort.active", { level: t(`effort.${value}`) })}`}
        className="flex items-center gap-0.5 rounded-md px-1.5 py-1 text-[13px] text-muted hover:bg-hover hover:text-fg"
      >
        <span>{t(`effort.${value}`)}</span>
        <ChevronDown size={13} />
      </button>
      {open && (
        <Menu up className="right-0 w-72">
          <div className="px-2 pt-1 pb-1.5">
            <div className="text-[11px] font-medium tracking-wide text-faint uppercase">
              {t("effort.label")} · {caps.display_name}
            </div>
            <div className="mt-0.5 font-mono text-[10.5px] text-faint">{EFFORT_PARAM[caps.provider]}</div>
          </div>
          {levels.map((l) => (
            <button
              key={l}
              type="button"
              onClick={() => {
                onChange(l);
                setOpen(false);
              }}
              className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left hover:bg-hover"
            >
              <span className="w-3.5">{l === value && <Check size={14} />}</span>
              <span className="flex-1 text-[13px]">{t(`effort.${l}`)}</span>
              {l === caps.default_effort && <span className="rounded border border-line px-1 text-[10px] text-faint">{t("effort.default")}</span>}
              <span className="font-mono text-[11px] text-faint">{effortWire(caps.provider, l)}</span>
            </button>
          ))}
          <div className="mt-1 space-y-1 border-t border-line px-2 pt-1.5 pb-1 text-[11px] leading-snug text-faint">
            <p>{t(NOTE[caps.provider])}</p>
            <p>{hint}</p>
          </div>
        </Menu>
      )}
    </div>
  );
}
