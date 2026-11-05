import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

// Apply the OS theme before first paint to avoid a white flash in dark mode.
if (window.matchMedia("(prefers-color-scheme: dark)").matches) document.documentElement.classList.add("dark");

// Block the webview's default context menu / reload shortcuts in release builds.
if (!import.meta.env.DEV) {
  window.addEventListener("contextmenu", (e) => {
    const el = e.target as HTMLElement;
    if (!el.closest("textarea, input, pre, .prose-y")) e.preventDefault();
  });
  window.addEventListener("keydown", (e) => {
    if (e.key === "F5" || (e.ctrlKey && (e.key === "r" || e.key === "R"))) e.preventDefault();
  });
}

async function start() {
  // Plain-browser preview during development only (tree-shaken from release builds).
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window && (window as { __TAURI_INTERNALS__?: { invoke?: unknown } }).__TAURI_INTERNALS__?.invoke)) {
    (await import("./dev/mock")).installMock();
  }
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}
start();
