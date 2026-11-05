import { useEffect, useState } from "react";
import { Download, X } from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { useT } from "../lib/i18n";

// Data URLs are fetched once per image id and reused across re-renders.
const cache = new Map<string, Promise<string>>();
function load(id: string) {
  let p = cache.get(id);
  if (!p) {
    p = api.readImage(id);
    p.catch(() => cache.delete(id));
    cache.set(id, p);
  }
  return p;
}

function Thumb({ id, onOpen }: { id: string; onOpen: (src: string) => void }) {
  const t = useT();
  const [src, setSrc] = useState<string | null>(null);
  const [missing, setMissing] = useState(false);
  const [saved, setSaved] = useState(false);
  useEffect(() => {
    let live = true;
    load(id).then((s) => live && setSrc(s), () => live && setMissing(true));
    return () => {
      live = false;
    };
  }, [id]);

  if (missing) return <div className="rounded-lg border border-line px-3 py-6 text-center text-xs text-faint">{t("image.missing")}</div>;
  return (
    <figure className="group relative m-0 inline-block overflow-hidden rounded-lg border border-line bg-code">
      {src ? (
        <button type="button" title={t("image.open")} onClick={() => onOpen(src)} className="block">
          <img src={src} alt={id} className="block max-h-[360px] max-w-full" />
        </button>
      ) : (
        <div className="pulse-dot h-[240px] w-[240px]" />
      )}
      {src && (
        <button
          type="button"
          onClick={async () => {
            const ext = id.split(".").pop() ?? "png";
            const dest = await save({ defaultPath: id, filters: [{ name: "Image", extensions: [ext] }] });
            if (dest) {
              await api.saveImageAs(id, dest);
              setSaved(true);
              setTimeout(() => setSaved(false), 1500);
            }
          }}
          className="absolute top-2 right-2 flex items-center gap-1 rounded-md border border-line bg-elev/90 px-2 py-1 text-[11px] opacity-0 shadow-sm group-hover:opacity-100 focus:opacity-100"
        >
          <Download size={12} />
          {saved ? t("image.saved") : t("image.saveAs")}
        </button>
      )}
    </figure>
  );
}

export function GeneratedImages({ ids }: { ids: string[] }) {
  const t = useT();
  const [full, setFull] = useState<string | null>(null);
  useEffect(() => {
    if (!full) return;
    const k = (e: KeyboardEvent) => e.key === "Escape" && setFull(null);
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [full]);
  return (
    <div className="flex flex-wrap gap-2 px-3 pb-3">
      {ids.map((id) => (
        <Thumb key={id} id={id} onOpen={setFull} />
      ))}
      {full && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-6" onClick={() => setFull(null)}>
          <img src={full} alt="" className="max-h-full max-w-full rounded-md shadow-2xl" onClick={(e) => e.stopPropagation()} />
          <button type="button" title={t("image.close")} onClick={() => setFull(null)} className="absolute top-4 right-4 rounded-full bg-black/50 p-2 text-white">
            <X size={18} />
          </button>
        </div>
      )}
    </div>
  );
}
