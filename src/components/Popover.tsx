import { useEffect, useRef, useState, type ReactNode } from "react";

/** Click-outside / Escape handling shared by the small dropdown menus. */
export function useDropdown() {
  const [open, setOpen] = useState(false);
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
  return { open, setOpen, ref };
}

export function Menu({ children, className = "", up = false }: { children: ReactNode; className?: string; up?: boolean }) {
  return (
    <div className={`fade-in absolute z-30 rounded-lg border border-line bg-elev p-1 shadow-lg ${up ? "bottom-full mb-1" : "top-full mt-1"} ${className}`}>
      {children}
    </div>
  );
}

export function MenuItem({ children, onClick, danger = false, active = false }: { children: ReactNode; onClick: () => void; danger?: boolean; active?: boolean }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[13px] hover:bg-hover ${danger ? "text-danger" : ""} ${active ? "font-medium" : ""}`}
    >
      {children}
    </button>
  );
}
