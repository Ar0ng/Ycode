import { useEffect } from "react";
import { useT } from "../lib/i18n";

export function ConfirmDialog({ message, onConfirm, onCancel, confirmLabel }: { message: string; onConfirm: () => void; onCancel: () => void; confirmLabel?: string }) {
  const t = useT();
  useEffect(() => {
    const k = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [onCancel]);
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/25" onMouseDown={onCancel}>
      <div role="alertdialog" aria-modal className="fade-in w-[360px] rounded-xl border border-line bg-elev p-4 shadow-xl" onMouseDown={(e) => e.stopPropagation()}>
        <p className="mb-4 text-[14px] leading-relaxed">{message}</p>
        <div className="flex justify-end gap-2">
          <button type="button" autoFocus onClick={onCancel} className="rounded-md border border-line px-3 py-1.5 text-xs hover:bg-hover">
            {t("confirm.cancel")}
          </button>
          <button type="button" onClick={onConfirm} className="rounded-md bg-danger px-3 py-1.5 text-xs font-medium text-white">
            {confirmLabel ?? t("confirm.ok")}
          </button>
        </div>
      </div>
    </div>
  );
}
