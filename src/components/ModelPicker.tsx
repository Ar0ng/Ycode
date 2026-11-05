import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown, Plus } from "lucide-react";
import { useStore } from "../lib/store";
import { useT } from "../lib/i18n";
import { api } from "../lib/api";
import type { ModelCaps, ProviderId } from "../lib/types";

const PROVIDERS: ProviderId[] = ["openai", "deepseek", "anthropic"];

export function ModelPicker({ provider, model, caps }: { provider: ProviderId; model: string; caps?: ModelCaps }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [manual, setManual] = useState("");
  const models = useStore((s) => s.models);
  const providers = useStore((s) => s.providers);
  const mode = useStore((s) => s.settings?.mode ?? "office");
  const selectModel = useStore((s) => s.selectModel);
  const setModels = useStore((s) => s.setModels);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    const esc = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    window.addEventListener("mousedown", close);
    window.addEventListener("keydown", esc);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("keydown", esc);
    };
  }, [open]);

  const addManual = async (p: ProviderId) => {
    const id = manual.trim();
    if (!id) return;
    try {
      setModels(await api.addManualModel(p, id));
      selectModel(p, id);
      setManual("");
      setOpen(false);
    } catch {
      /* invalid id: keep the field for correction */
    }
  };

  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        className="flex max-w-[220px] items-center gap-1 rounded-md px-2 py-1 text-xs text-muted hover:bg-hover hover:text-fg"
        title={t("model.pick")}
      >
        <span className="truncate font-medium text-fg">{caps?.display_name ?? model ?? "—"}</span>
        <ChevronDown size={13} />
      </button>
      {open && (
        <div className="fade-in absolute bottom-full left-0 z-30 mb-1 max-h-[60vh] w-80 overflow-y-auto rounded-lg border border-line bg-elev p-1 shadow-lg">
          {PROVIDERS.map((p) => {
            const list = models.filter((m) => m.provider === p);
            const info = providers.find((x) => x.id === p);
            return (
              <div key={p} className="py-1">
                <div className="flex items-center justify-between px-2 py-1 text-[11px] font-medium tracking-wide text-faint uppercase">
                  <span>{t(`provider.${p}`)}</span>
                  {!info?.has_key && <span className="normal-case">{t("provider.keyMissing")}</span>}
                </div>
                {list.map((m) => {
                  const active = p === provider && m.id === model;
                  return (
                    <button
                      key={m.id}
                      type="button"
                      onClick={() => {
                        selectModel(p, m.id);
                        setOpen(false);
                      }}
                      className={`flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left hover:bg-hover ${m.available === false ? "opacity-55" : ""}`}
                    >
                      <span className="mt-0.5 w-3.5">{active && <Check size={14} />}</span>
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-[13px] text-fg">{m.display_name}</span>
                        <span className="block truncate font-mono text-[11px] text-faint">
                          {m.id}
                          {m.recommended.includes(mode) && ` · ${t("model.recommended")}`}
                          {m.source === "manual" && ` · ${t("model.manual")}`}
                          {(m.source === "unknown" || m.source === "manual") && ` · ${t("model.unknownCaps")}`}
                          {m.available === false && ` · ${t("model.unavailable")}`}
                        </span>
                      </span>
                    </button>
                  );
                })}
              </div>
            );
          })}
          <div className="flex gap-1 border-t border-line p-1.5">
            <input
              value={manual}
              onChange={(e) => setManual(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && addManual(provider)}
              placeholder={`${t("model.addManual")} (${t(`provider.${provider}`)})`}
              className="min-w-0 flex-1 rounded-md border border-line bg-bg px-2 py-1 font-mono text-xs outline-none"
            />
            <button type="button" onClick={() => addManual(provider)} className="rounded-md px-1.5 text-muted hover:bg-hover hover:text-fg" title={t("provider.add")}>
              <Plus size={15} />
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
