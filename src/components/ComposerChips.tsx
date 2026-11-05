import { Briefcase, Check, ChevronDown, Code2, ShieldCheck } from "lucide-react";
import { useStore } from "../lib/store";
import { useT } from "../lib/i18n";
import type { Mode } from "../lib/types";
import { Menu, useDropdown } from "./Popover";

function Option({ active, title, desc, icon, onClick }: { active: boolean; title: string; desc: string; icon: React.ReactNode; onClick: () => void }) {
  return (
    <button type="button" onClick={onClick} className="flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left hover:bg-hover">
      <span className="mt-0.5 text-muted">{icon}</span>
      <span className="min-w-0 flex-1">
        <span className="block text-[13px]">{title}</span>
        <span className="block text-[11.5px] leading-snug text-faint">{desc}</span>
      </span>
      <span className="mt-0.5 w-3.5">{active && <Check size={14} />}</span>
    </button>
  );
}

/** "办公模式 / 编程模式" dropdown shown above the input on a new chat. */
export function ModeChip() {
  const t = useT();
  const mode = useStore((s) => s.settings?.mode ?? "office");
  const setMode = useStore((s) => s.setMode);
  const { open, setOpen, ref } = useDropdown();
  const icon = (m: Mode) => (m === "coding" ? <Code2 size={14} /> : <Briefcase size={14} />);
  return (
    <div className="relative" ref={ref}>
      <button type="button" onClick={() => setOpen((o) => !o)} className="flex items-center gap-1.5 rounded-md px-2 py-1 text-[13px] font-semibold hover:bg-hover">
        {icon(mode)}
        {t(mode === "coding" ? "mode.codingLong" : "mode.officeLong")}
        <ChevronDown size={13} className="text-faint" />
      </button>
      {open && (
        <Menu className="left-0 w-72">
          {(["office", "coding"] as Mode[]).map((m) => (
            <Option
              key={m}
              active={m === mode}
              icon={icon(m)}
              title={t(m === "coding" ? "mode.codingLong" : "mode.officeLong")}
              desc={t(m === "coding" ? "welcome.coding" : "welcome.office")}
              onClick={() => {
                setMode(m);
                setOpen(false);
              }}
            />
          ))}
        </Menu>
      )}
    </div>
  );
}

/** Approval mode: ask before edits, or apply edits inside the workspace automatically. */
export function ApprovalChip() {
  const t = useT();
  const auto = useStore((s) => s.settings?.auto_approve_edits ?? false);
  const patch = useStore((s) => s.patchSettings);
  const { open, setOpen, ref } = useDropdown();
  const pick = (v: boolean) => {
    patch({ auto_approve_edits: v });
    setOpen(false);
  };
  return (
    <div className="relative" ref={ref}>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        title={t(auto ? "approval.autoDesc" : "approval.askDesc")}
        className="flex items-center gap-1 rounded-md px-2 py-1 text-[13px] text-muted hover:bg-hover hover:text-fg"
      >
        <ShieldCheck size={14} />
        {t(auto ? "approval.auto" : "approval.ask")}
        <ChevronDown size={13} />
      </button>
      {open && (
        <Menu up className="left-0 w-72">
          <Option active={!auto} icon={<ShieldCheck size={14} />} title={t("approval.ask")} desc={t("approval.askDesc")} onClick={() => pick(false)} />
          <Option active={auto} icon={<ShieldCheck size={14} />} title={t("approval.auto")} desc={t("approval.autoDesc")} onClick={() => pick(true)} />
        </Menu>
      )}
    </div>
  );
}
