import { memo, useState, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkGfm from "remark-gfm";
import { Check, Copy } from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useT } from "../lib/i18n";

function textOf(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(textOf).join("");
  if (node && typeof node === "object" && "props" in node) return textOf((node as { props: { children?: ReactNode } }).props.children);
  return "";
}

export function CopyButton({ text, className = "" }: { text: string; className?: string }) {
  const t = useT();
  const [done, setDone] = useState(false);
  return (
    <button
      type="button"
      title={t("chat.copy")}
      className={`inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-xs text-muted hover:bg-hover hover:text-fg ${className}`}
      onClick={() => {
        navigator.clipboard.writeText(text).then(() => {
          setDone(true);
          setTimeout(() => setDone(false), 1200);
        });
      }}
    >
      {done ? <Check size={13} /> : <Copy size={13} />}
      {done ? t("chat.copied") : t("chat.copy")}
    </button>
  );
}

const components: Components = {
  pre({ children }) {
    const child = Array.isArray(children) ? children[0] : children;
    const cls = (child as { props?: { className?: string } })?.props?.className ?? "";
    const lang = /language-([\w+-]+)/.exec(cls)?.[1] ?? "";
    const code = textOf(children).replace(/\n$/, "");
    return (
      <div className="my-2 overflow-hidden rounded-lg border border-line bg-code">
        <div className="flex items-center justify-between border-b border-line px-3 py-1 text-xs text-faint">
          <span>{lang || "text"}</span>
          <CopyButton text={code} />
        </div>
        <pre className="m-0! overflow-x-auto px-3 py-2.5 font-mono text-[12.5px] leading-relaxed">{children}</pre>
      </div>
    );
  },
  a({ href, children }) {
    // Links open in the system browser; the app window never navigates away.
    return (
      <a
        href={href}
        onClick={(e) => {
          e.preventDefault();
          if (href && /^https?:\/\//i.test(href)) openUrl(href).catch(() => {});
        }}
      >
        {children}
      </a>
    );
  },
  img({ alt }) {
    // Remote images are not loaded (CSP + privacy); show the alt text instead.
    return <span className="text-muted">[{alt || "image"}]</span>;
  },
};

export const Markdown = memo(function Markdown({ text }: { text: string }) {
  return (
    <div className="prose-y">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
        {text}
      </ReactMarkdown>
    </div>
  );
});
