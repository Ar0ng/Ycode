import { useEffect, useRef, useState } from "react";
import { X } from "lucide-react";
import { useStore } from "../lib/store";
import { useT } from "../lib/i18n";
import type { AppError } from "../lib/types";
import { Avatar } from "./Avatar";

/** Center-crop and downscale a picked image to a 256×256 PNG data URL. */
async function toAvatarDataUrl(file: File): Promise<string> {
  const url = URL.createObjectURL(file);
  try {
    const img = await new Promise<HTMLImageElement>((resolve, reject) => {
      const i = new Image();
      i.onload = () => resolve(i);
      i.onerror = () => reject(new Error("bad image"));
      i.src = url;
    });
    const side = Math.min(img.naturalWidth, img.naturalHeight);
    if (!side) throw new Error("empty image");
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 256;
    const ctx = canvas.getContext("2d")!;
    ctx.imageSmoothingQuality = "high";
    ctx.drawImage(img, (img.naturalWidth - side) / 2, (img.naturalHeight - side) / 2, side, side, 0, 0, 256, 256);
    return canvas.toDataURL("image/png");
  } finally {
    URL.revokeObjectURL(url);
  }
}

export function ProfileDialog({ onClose }: { onClose: () => void }) {
  const t = useT();
  const settings = useStore((s) => s.settings)!;
  const osUser = useStore((s) => s.osUser);
  const savedAvatar = useStore((s) => s.avatar);
  const { patchSettings, setAvatar } = useStore.getState();
  const [name, setName] = useState(settings.profile_name);
  const [avatar, setLocalAvatar] = useState<string | null>(savedAvatar);
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const file = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const k = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [onClose]);

  const save = async () => {
    setBusy(true);
    setErr(null);
    try {
      if (avatar !== savedAvatar) await setAvatar(avatar);
      if (name.trim() !== settings.profile_name) await patchSettings({ profile_name: name.trim() });
      onClose();
    } catch (e) {
      setErr((e as AppError)?.message ?? String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/25" onMouseDown={onClose}>
      <div role="dialog" aria-modal aria-label={t("profile.title")} className="fade-in w-[400px] rounded-xl border border-line bg-elev p-5 shadow-xl" onMouseDown={(e) => e.stopPropagation()}>
        <div className="mb-4 flex items-center justify-between">
          <h3 className="text-[15px] font-semibold">{t("profile.title")}</h3>
          <button type="button" onClick={onClose} title={t("settings.close")} className="rounded-md p-1 text-muted hover:bg-hover hover:text-fg">
            <X size={16} />
          </button>
        </div>
        <div className="mb-1.5 text-[12px] text-muted">{t("profile.avatar")}</div>
        <div className="mb-4 flex items-center gap-3">
          <Avatar src={avatar} name={name || osUser} size={64} />
          <button type="button" onClick={() => file.current?.click()} className="rounded-md border border-line px-3 py-1.5 text-xs hover:bg-hover">
            {t("profile.changeAvatar")}
          </button>
          {avatar && (
            <button type="button" onClick={() => setLocalAvatar(null)} className="rounded-md px-2 py-1.5 text-xs text-muted hover:bg-hover hover:text-fg">
              {t("profile.removeAvatar")}
            </button>
          )}
          <input
            ref={file}
            type="file"
            accept="image/png,image/jpeg,image/webp"
            className="hidden"
            onChange={async (e) => {
              const f = e.target.files?.[0];
              e.target.value = "";
              if (!f) return;
              try {
                setLocalAvatar(await toAvatarDataUrl(f));
                setErr(null);
              } catch {
                setErr(t("profile.badImage"));
              }
            }}
          />
        </div>
        <label className="mb-1.5 block text-[12px] text-muted" htmlFor="profile-name">
          {t("profile.name")}
        </label>
        <input
          id="profile-name"
          autoFocus
          value={name}
          maxLength={32}
          placeholder={osUser || t("profile.namePlaceholder")}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && save()}
          className="w-full rounded-md border border-line bg-bg px-2.5 py-1.5 text-[13px] outline-none focus:border-line-strong"
        />
        <p className="mt-1.5 text-[11px] text-faint">{t("profile.nameHint", { os: osUser || "—" })}</p>
        {err && <p className="mt-2 text-xs text-danger">{err}</p>}
        <div className="mt-5 flex justify-end gap-2">
          <button type="button" onClick={onClose} className="rounded-md border border-line px-3 py-1.5 text-xs hover:bg-hover">
            {t("confirm.cancel")}
          </button>
          <button type="button" disabled={busy} onClick={save} className="rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-accent-fg disabled:opacity-50">
            {t("profile.save")}
          </button>
        </div>
      </div>
    </div>
  );
}
