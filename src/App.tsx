import { useEffect, useMemo } from "react";
import { X } from "lucide-react";
import { displayName, useStore } from "./lib/store";
import { pickGreeting } from "./lib/greeting";
import { useT, type Key } from "./lib/i18n";
import { Sidebar } from "./components/Sidebar";
import { Composer } from "./components/Composer";
import { MessageList, ErrorBox } from "./components/MessageList";
import { SettingsView } from "./components/Settings";
import { UsageLine } from "./components/UsageLine";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { Logo } from "./components/Logo";
import { TopBar } from "./components/TopBar";

function useTheme() {
  const theme = useStore((s) => s.settings?.theme ?? "system");
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => document.documentElement.classList.toggle("dark", theme === "dark" || (theme === "system" && mq.matches));
    apply();
    mq.addEventListener("change", apply);
    return () => mq.removeEventListener("change", apply);
  }, [theme]);
}

function Notices() {
  const t = useT();
  const notices = useStore((s) => s.notices);
  const dismiss = useStore((s) => s.dismissNotice);
  if (!notices.length) return null;
  return (
    <div className="pointer-events-none fixed top-3 right-3 z-40 flex w-80 flex-col gap-2">
      {notices.map((n) => (
        <div key={n.id} className="fade-in pointer-events-auto flex items-start gap-2 rounded-lg border border-line bg-elev px-3 py-2 text-xs shadow-md">
          <span className="flex-1 leading-relaxed">{n.code === "update" ? n.detail : t(`notice.${n.code}` as Key, { detail: n.detail })}</span>
          <button type="button" onClick={() => dismiss(n.id)} className="text-faint hover:text-fg">
            <X size={13} />
          </button>
        </div>
      ))}
    </div>
  );
}

function Welcome() {
  const t = useT();
  const mode = useStore((s) => s.settings?.mode ?? "office");
  const lang = useStore((s) => s.lang);
  const name = useStore((s) => displayName(s));
  const workspace = useStore((s) => s.settings?.workspace ?? null);
  const draftId = useStore((s) => s.draftId);
  // One random pick per new chat (draftId), so it doesn't change while typing.
  const roll = useMemo(() => Math.random(), [draftId]);
  const greeting = useMemo(() => pickGreeting({ lang, name, workspace, now: new Date(), random: roll }), [lang, name, workspace, roll]);
  return (
    <div className="mb-8 text-center select-none">
      <div className="mb-3 flex items-center justify-center gap-3">
        <Logo size={44} />
        <span className="text-[34px] font-semibold tracking-tight">Ycode</span>
      </div>
      <div className="text-[17px] text-fg">{greeting}</div>
      <div className="mt-1.5 text-[13px] text-faint">{mode === "coding" ? t("welcome.coding") : t("welcome.office")}</div>
    </div>
  );
}

function ChatArea() {
  const t = useT();
  const currentId = useStore((s) => s.currentId);
  const hasMessages = useStore((s) => s.messages.length > 0);
  const loading = useStore((s) => s.loadingConversation);
  const convUsage = useStore((s) => s.convUsage);
  const lastUsage = useStore((s) => s.lastUsage);
  const lastError = useStore((s) => s.lastError);
  const isNew = currentId === null && !hasMessages;

  if (isNew) {
    return (
      <div className="flex min-h-0 flex-1 flex-col">
        <div className="flex min-h-0 flex-1 flex-col items-center justify-center px-6 pb-[10vh]">
          <Welcome />
          <Composer centered />
          {lastError && (
            <div className="mt-3 w-full max-w-[720px]">
              <ErrorBox error={lastError} />
            </div>
          )}
        </div>
      </div>
    );
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <TopBar isNew={false} />
      {loading ? <div className="flex-1" /> : <MessageList />}
      <div className="px-6 pt-1 pb-3">
        <Composer centered={false} />
        {(lastUsage || convUsage) && (
          <div className="mx-auto mt-0.5 flex max-w-[800px] flex-wrap justify-center gap-x-4 text-[11px] text-faint">
            {lastUsage && <UsageLine usage={lastUsage} label={t("usage.last")} />}
            {convUsage && <UsageLine usage={convUsage} label={t("usage.total")} />}
          </div>
        )}
      </div>
    </div>
  );
}

export default function App() {
  const ready = useStore((s) => s.ready);
  const bootError = useStore((s) => s.bootError);
  const view = useStore((s) => s.view);
  const init = useStore((s) => s.init);
  useTheme();

  useEffect(() => {
    init();
  }, [init]);


  if (bootError) return <div className="p-8 text-[13px] text-danger">Ycode failed to start: {bootError}</div>;
  if (!ready) return <div className="h-full bg-bg" />;

  return (
    <div className="flex h-full min-w-[900px]">
      <Sidebar />
      <main className="flex min-w-0 flex-1 flex-col bg-panel">
        <ErrorBoundary resetKey={view} onBack={() => useStore.getState().setView("chat")}>
          {view === "settings" ? <SettingsView /> : <ChatArea />}
        </ErrorBoundary>
      </main>
      <Notices />
    </div>
  );
}
