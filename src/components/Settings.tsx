import { useMemo, useState } from "react";
import { Check, Eye, EyeOff, FolderOpen, Loader2, RefreshCw, Trash2, X } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { relaunch } from "@tauri-apps/plugin-process";
import { openPath } from "@tauri-apps/plugin-opener";
import { useStore } from "../lib/store";
import { useT, type Key } from "../lib/i18n";
import { api } from "../lib/api";
import type { AppError, ProviderId, UpdateInfo } from "../lib/types";
import { ConfirmDialog } from "./ConfirmDialog";
import { isLegacy, modelKey } from "../lib/models";
import { displayVersion } from "../lib/format";

// Keep in sync with images::KNOWN_MODELS (src-tauri/src/images.rs).
const IMAGE_MODELS = ["gpt-image-2.6-flare", "gpt-image-2.6-sunburst", "gpt-image-2.5-flare", "gpt-image-2.5-sunburst"];

type Tab = "general" | "providers" | "skills" | "updates" | "about";

function errText(t: (k: Key, v?: Record<string, string | number>) => string, e: unknown) {
  const err = e as AppError;
  if (err?.provider) return `${t(`err.${err.provider.kind}` as Key)}${err.provider.detail ? ` (${err.provider.detail})` : ""}`;
  return err?.message ?? String(e);
}

function Row({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line py-3.5 last:border-0">
      <div className="min-w-[200px] flex-1">
        <div className="text-[13px]">{label}</div>
        {hint && <div className="mt-0.5 text-xs text-faint">{hint}</div>}
      </div>
      <div>{children}</div>
    </div>
  );
}

const inputCls = "rounded-md border border-line bg-bg px-2.5 py-1.5 text-[13px] outline-none focus:border-line-strong";
const btnCls = "inline-flex items-center gap-1.5 rounded-md border border-line px-3 py-1.5 text-xs hover:bg-hover disabled:opacity-50";

function General() {
  const t = useT();
  const s = useStore((x) => x.settings)!;
  const patch = useStore((x) => x.patchSettings);
  return (
    <div>
      <Row label={t("settings.language")}>
        <select className={inputCls} value={s.language ?? "en-US"} onChange={(e) => patch({ language: e.target.value })}>
          <option value="zh-CN">简体中文</option>
          <option value="en-US">English</option>
        </select>
      </Row>
      <Row label={t("settings.theme")}>
        <select className={inputCls} value={s.theme} onChange={(e) => patch({ theme: e.target.value as "system" | "light" | "dark" })}>
          <option value="system">{t("settings.theme.system")}</option>
          <option value="light">{t("settings.theme.light")}</option>
          <option value="dark">{t("settings.theme.dark")}</option>
        </select>
      </Row>
      <Row label={t("settings.maxSteps")}>
        <input type="number" min={1} max={200} className={`${inputCls} w-24`} value={s.max_steps} onChange={(e) => patch({ max_steps: Math.max(1, Math.min(200, Number(e.target.value) || 1)) })} />
      </Row>
      <Row label={t("settings.fallbacks")} hint={t("settings.fallbacksHint")}>
        <input type="checkbox" checked={s.claude_fallbacks} onChange={(e) => patch({ claude_fallbacks: e.target.checked })} />
      </Row>
      <Row label={t("settings.images")} hint={t("settings.imagesHint")}>
        <input type="checkbox" checked={s.image_generation} onChange={(e) => patch({ image_generation: e.target.checked })} />
      </Row>
      <Row label={t("settings.imageModel")}>
        <select className={inputCls} disabled={!s.image_generation} value={s.image_model} onChange={(e) => patch({ image_model: e.target.value })}>
          {(IMAGE_MODELS.includes(s.image_model) ? IMAGE_MODELS : [s.image_model, ...IMAGE_MODELS]).map((m) => (
            <option key={m} value={m}>
              {m}
            </option>
          ))}
        </select>
      </Row>
    </div>
  );
}

function ProviderCard({ id }: { id: ProviderId }) {
  const t = useT();
  const info = useStore((s) => s.providers.find((p) => p.id === id));
  // Select the stable array and derive locally: a selector that returns a new
  // array on every call makes zustand re-render forever (blank window).
  const allModels = useStore((s) => s.models);
  const models = useMemo(() => allModels.filter((m) => m.provider === id), [allModels, id]);
  const hideLegacy = useStore((s) => s.settings?.hide_legacy_models ?? true);
  const hidden = useStore((s) => s.settings?.hidden_models);
  const patch = useStore((s) => s.patchSettings);
  const toggleHidden = (modelId: string) => {
    const k = modelKey(id, modelId);
    const cur = hidden ?? [];
    patch({ hidden_models: cur.includes(k) ? cur.filter((x) => x !== k) : [...cur, k] });
  };
  const { setProviders, setModels } = useStore.getState();
  const [key, setKey] = useState("");
  const [url, setUrl] = useState(info?.base_url ?? "");
  const [busy, setBusy] = useState(false);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);
  const [confirm, setConfirm] = useState(false);

  const act = async (f: () => Promise<void>) => {
    setBusy(true);
    setMsg(null);
    try {
      await f();
    } catch (e) {
      setMsg({ ok: false, text: errText(t, e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="mb-5 rounded-xl border border-line p-4">
      <div className="mb-3 flex items-center justify-between">
        <h3 className="text-[14px] font-semibold">{t(`provider.${id}`)}</h3>
        <span className={`text-xs ${info?.has_key ? "text-ok" : "text-faint"}`}>{info?.has_key ? t("provider.keySaved") : t("provider.keyMissing")}</span>
      </div>
      <div className="mb-2 flex gap-2">
        <input
          type="password"
          autoComplete="off"
          spellCheck={false}
          placeholder={t("provider.keyPlaceholder")}
          value={key}
          onChange={(e) => setKey(e.target.value)}
          className={`${inputCls} min-w-0 flex-1 font-mono`}
        />
        <button
          type="button"
          disabled={!key.trim() || busy}
          className={btnCls}
          onClick={() =>
            act(async () => {
              setProviders(await api.setApiKey(id, key));
              setKey("");
            })
          }
        >
          {t("provider.save")}
        </button>
        {info?.has_key && (
          <button type="button" className={`${btnCls} text-danger`} onClick={() => setConfirm(true)}>
            {t("provider.deleteKey")}
          </button>
        )}
      </div>
      <div className="mb-3 flex gap-2">
        <input
          placeholder={`${t("provider.baseUrl")} · ${info?.default_base_url ?? ""}`}
          title={t("provider.baseUrlHint")}
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onBlur={() => url !== (info?.base_url ?? "") && act(async () => setProviders(await api.setBaseUrl(id, url)))}
          className={`${inputCls} min-w-0 flex-1 font-mono text-xs`}
        />
        <button
          type="button"
          disabled={busy || !info?.has_key}
          className={btnCls}
          onClick={() =>
            act(async () => {
              const r = await api.refreshModels(id);
              setModels(r.models);
              setMsg({ ok: true, text: t("provider.testOk", { n: r.returned }) });
              setProviders(await api.bootstrap().then((b) => b.providers));
            })
          }
        >
          {busy ? <Loader2 size={13} className="animate-spin" /> : <RefreshCw size={13} />}
          {t("provider.test")}
        </button>
      </div>
      {msg && <div className={`mb-2 text-xs ${msg.ok ? "text-ok" : "text-danger"}`}>{msg.text}</div>}
      {info?.refreshed_at && <div className="mb-2 text-[11px] text-faint">{t("provider.refreshedAt", { time: new Date(info.refreshed_at).toLocaleString() })}</div>}
      <div className="text-[11px] font-medium tracking-wide text-faint uppercase">{t("provider.models")}</div>
      <ul className="mt-1 max-h-56 overflow-y-auto">
        {models.map((m) => {
          const legacy = isLegacy(m);
          const manuallyHidden = (hidden ?? []).includes(modelKey(id, m.id));
          const isHiddenNow = manuallyHidden || (hideLegacy && legacy);
          return (
          <li key={m.id} className={`flex items-center gap-2 py-1 text-xs ${isHiddenNow ? "opacity-50" : ""}`}>
            <button
              type="button"
              title={manuallyHidden ? t("model.show") : t("model.hide")}
              disabled={hideLegacy && legacy && !manuallyHidden}
              onClick={() => toggleHidden(m.id)}
              className="text-faint hover:text-fg disabled:cursor-default disabled:hover:text-faint"
            >
              {isHiddenNow ? <EyeOff size={13} /> : <Eye size={13} />}
            </button>
            <span className={`font-mono ${m.available === false ? "text-faint" : ""}`}>{m.id}</span>
            {legacy && <span className="rounded border border-line px-1 text-[10px] text-faint">{t("model.legacy")}</span>}
            <span className="text-faint">
              {m.efforts.length ? m.efforts.map((e) => t(`effort.${e}` as Key)).join(" / ") : t("model.unknownCaps")}
              {m.available === false && ` · ${t("model.unavailable")}`}
            </span>
            {m.source === "manual" && (
              <button type="button" title={t("provider.remove")} className="ml-auto text-faint hover:text-danger" onClick={() => act(async () => setModels(await api.removeManualModel(id, m.id)))}>
                <X size={12} />
              </button>
            )}
          </li>
          );
        })}
      </ul>
      {confirm && (
        <ConfirmDialog
          message={t("confirm.deleteKey", { provider: t(`provider.${id}`) })}
          onCancel={() => setConfirm(false)}
          onConfirm={() => {
            setConfirm(false);
            act(async () => setProviders(await api.deleteApiKey(id)));
          }}
        />
      )}
    </section>
  );
}

function Skills() {
  const t = useT();
  const skills = useStore((s) => s.skills);
  const setSkills = useStore((s) => s.setSkills);
  const [err, setErr] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);
  const [overwrite, setOverwrite] = useState<string | null>(null);

  const install = async (path: string, force: boolean) => {
    setErr(null);
    try {
      setSkills(await api.installSkill(path, force));
    } catch (e) {
      const m = errText(t, e);
      if (!force && m.includes("already installed")) setOverwrite(path);
      else setErr(m);
    }
  };

  return (
    <div>
      <div className="mb-3 flex items-center gap-3">
        <button
          type="button"
          className={btnCls}
          onClick={async () => {
            const dir = await openDialog({ directory: true, multiple: false });
            if (typeof dir === "string") install(dir, false);
          }}
        >
          <FolderOpen size={13} />
          {t("skills.install")}
        </button>
        <span className="text-xs text-faint">{t("skills.hint")}</span>
      </div>
      {err && <div className="mb-3 text-xs text-danger">{err}</div>}
      {skills.length === 0 && <div className="text-[13px] text-faint">{t("skills.empty")}</div>}
      {skills.map((s) => (
        <div key={s.name} className="mb-2 rounded-lg border border-line p-3">
          <div className="flex items-center gap-2">
            <span className="font-mono text-[13px] font-medium">{s.name}</span>
            {s.has_scripts && <span className="rounded border border-line px-1.5 text-[10px] text-faint">{t("skills.scripts")}</span>}
            <span className="flex-1" />
            <label className="flex items-center gap-1 text-xs text-muted">
              <input type="checkbox" checked={s.enabled} disabled={!s.description} onChange={async (e) => setSkills(await api.setSkillEnabled(s.name, e.target.checked))} />
              {t("skills.enabled")}
            </label>
            <button type="button" title={t("skills.delete")} onClick={() => setConfirm(s.name)} className="rounded p-1 text-faint hover:text-danger">
              <Trash2 size={13} />
            </button>
          </div>
          {s.description && <p className="mt-1 text-xs leading-relaxed text-muted">{s.description}</p>}
          {s.warnings.map((w) => (
            <p key={w} className="mt-1 text-[11px] text-warn">
              {w}
            </p>
          ))}
        </div>
      ))}
      {confirm && (
        <ConfirmDialog
          message={t("confirm.deleteSkill", { name: confirm })}
          onCancel={() => setConfirm(null)}
          onConfirm={async () => {
            const n = confirm;
            setConfirm(null);
            try {
              setSkills(await api.deleteSkill(n));
            } catch (e) {
              setErr(errText(t, e));
            }
          }}
        />
      )}
      {overwrite && (
        <ConfirmDialog
          message={t("skills.overwrite")}
          confirmLabel={t("skills.install").replace("…", "")}
          onCancel={() => setOverwrite(null)}
          onConfirm={() => {
            const p = overwrite;
            setOverwrite(null);
            install(p, true);
          }}
        />
      )}
    </div>
  );
}

function Updates() {
  const t = useT();
  const s = useStore((x) => x.settings)!;
  const patch = useStore((x) => x.patchSettings);
  const [repo, setRepo] = useState(s.update_repo);
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const [state, setState] = useState<"idle" | "checking" | "downloading" | "done">("idle");
  const [pct, setPct] = useState("");
  const [err, setErr] = useState<string | null>(null);

  const check = async () => {
    setErr(null);
    setInfo(null);
    setState("checking");
    try {
      setInfo(await api.checkUpdate());
    } catch (e) {
      setErr(errText(t, e));
    }
    setState("idle");
  };

  return (
    <div>
      <Row label={t("updates.repo")} hint={t("updates.repoHint")}>
        <input value={repo} placeholder="owner/Ycode" onChange={(e) => setRepo(e.target.value)} onBlur={() => repo !== s.update_repo && patch({ update_repo: repo.trim() })} className={`${inputCls} w-56 font-mono text-xs`} />
      </Row>
      <Row label={t("updates.onStart")}>
        <input type="checkbox" checked={s.check_updates_on_start} onChange={(e) => patch({ check_updates_on_start: e.target.checked })} />
      </Row>
      <div className="mt-4 flex items-center gap-3">
        <button type="button" className={btnCls} disabled={state !== "idle" || !s.update_repo} onClick={check}>
          {state === "checking" ? <Loader2 size={13} className="animate-spin" /> : <RefreshCw size={13} />}
          {t("updates.check")}
        </button>
        {!s.update_repo && <span className="text-xs text-faint">{t("updates.notConfigured")}</span>}
      </div>
      {err && <div className="mt-3 text-xs text-danger">{err}</div>}
      {info && !info.available && (
        <div className="mt-3 flex items-center gap-1.5 text-xs text-ok">
          <Check size={13} />
          {t("updates.none", { version: displayVersion(info.current_version) })}
        </div>
      )}
      {info?.available && (
        <div className="mt-3 rounded-lg border border-line p-3">
          <div className="mb-1 text-[13px] font-medium">{t("updates.available", { version: displayVersion(info.version) })}</div>
          {info.notes && <pre className="mb-2 max-h-48 overflow-auto font-sans text-xs whitespace-pre-wrap text-muted">{info.notes}</pre>}
          {state === "done" ? (
            <div className="flex items-center gap-3 text-xs">
              {t("updates.restart")}
              <button type="button" className={btnCls} onClick={() => relaunch()}>
                {t("updates.restartNow")}
              </button>
            </div>
          ) : (
            <button
              type="button"
              className={btnCls}
              disabled={state === "downloading"}
              onClick={async () => {
                setState("downloading");
                setErr(null);
                try {
                  await api.installUpdate((p) => setPct(p.total ? `${Math.round((p.downloaded / p.total) * 100)}%` : `${(p.downloaded / 1048576).toFixed(1)} MB`));
                  setState("done");
                } catch (e) {
                  setErr(errText(t, e));
                  setState("idle");
                }
              }}
            >
              {state === "downloading" ? t("updates.downloading", { pct }) : t("updates.install")}
            </button>
          )}
        </div>
      )}
    </div>
  );
}

function About() {
  const t = useT();
  const version = useStore((s) => s.version);
  const [dir, setDir] = useState<string | null>(null);
  return (
    <div className="text-[13px]">
      <p className="mb-2 font-medium">Ycode</p>
      <p className="mb-4 text-muted">{t("about.version", { v: displayVersion(version) })}</p>
      <p className="mb-4 text-muted">{t("about.privacy")}</p>
      <button
        type="button"
        className={btnCls}
        onClick={async () => {
          const d = await api.dataFolder();
          setDir(d);
          openPath(d).catch(() => {});
        }}
      >
        <FolderOpen size={13} />
        {t("about.data")}
      </button>
      {dir && <div className="mt-2 font-mono text-xs break-all text-faint">{dir}</div>}
    </div>
  );
}

function LegacyToggle() {
  const t = useT();
  const s = useStore((x) => x.settings)!;
  const patch = useStore((x) => x.patchSettings);
  const models = useStore((x) => x.models);
  const n = useMemo(
    () => models.filter((m) => s.hidden_models.includes(modelKey(m.provider, m.id)) || (s.hide_legacy_models && isLegacy(m))).length,
    [models, s.hidden_models, s.hide_legacy_models],
  );
  return (
    <div className="mb-4 rounded-xl border border-line px-4">
      <Row label={t("settings.hideLegacy")} hint={`${t("settings.hideLegacyHint")} ${t("model.hiddenCount", { n })}`}>
        <input type="checkbox" checked={s.hide_legacy_models} onChange={(e) => patch({ hide_legacy_models: e.target.checked })} />
      </Row>
    </div>
  );
}

export function SettingsView() {
  const t = useT();
  const [tab, setTab] = useState<Tab>("general");
  const tabs: Tab[] = ["general", "providers", "skills", "updates", "about"];
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center justify-between border-b border-line px-6 py-3">
        <h2 className="text-[15px] font-semibold">{t("settings.title")}</h2>
        <button type="button" onClick={() => useStore.getState().setView("chat")} title={t("settings.close")} className="rounded-md p-1 text-muted hover:bg-hover hover:text-fg">
          <X size={17} />
        </button>
      </div>
      <div className="flex min-h-0 flex-1">
        <nav className="w-44 shrink-0 space-y-0.5 p-3">
          {tabs.map((x) => (
            <button key={x} type="button" onClick={() => setTab(x)} className={`block w-full rounded-md px-2.5 py-1.5 text-left text-[13px] ${tab === x ? "bg-hover font-medium" : "text-muted hover:bg-hover/70"}`}>
              {t(`settings.${x}`)}
            </button>
          ))}
        </nav>
        <div className="min-w-0 flex-1 overflow-y-auto px-6 py-4">
          <div className="max-w-[680px]">
            {tab === "general" && <General />}
            {tab === "providers" && (
              <>
                <LegacyToggle />
                {(["openai", "deepseek", "anthropic"] as ProviderId[]).map((p) => (
                  <ProviderCard key={p} id={p} />
                ))}
              </>
            )}
            {tab === "skills" && <Skills />}
            {tab === "updates" && <Updates />}
            {tab === "about" && <About />}
          </div>
        </div>
      </div>
    </div>
  );
}
