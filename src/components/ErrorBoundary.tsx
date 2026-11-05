import { Component, type ReactNode } from "react";
import { translate, type Lang } from "../lib/i18n";
import { useStore } from "../lib/store";

interface Props {
  children: ReactNode;
  /** Changing this value clears the error (e.g. switching view/tab). */
  resetKey?: unknown;
  onBack?: () => void;
}

/**
 * Keeps a render error in one area from blanking the whole window: shows a short
 * message with a way back instead.
 */
export class ErrorBoundary extends Component<Props, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidUpdate(prev: Props) {
    if (prev.resetKey !== this.props.resetKey && this.state.error) this.setState({ error: null });
  }

  render() {
    if (!this.state.error) return this.props.children;
    const lang = useStore.getState().lang as Lang;
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-3 p-8 text-center">
        <div className="text-[14px] font-medium">{translate(lang, "err.render")}</div>
        <div className="max-w-[560px] font-mono text-xs break-all text-faint">{this.state.error.message}</div>
        <div className="flex gap-2">
          {this.props.onBack && (
            <button type="button" onClick={this.props.onBack} className="rounded-md bg-accent px-3 py-1.5 text-xs font-medium text-accent-fg">
              {translate(lang, "err.back")}
            </button>
          )}
          <button type="button" onClick={() => this.setState({ error: null })} className="rounded-md border border-line px-3 py-1.5 text-xs hover:bg-hover">
            {translate(lang, "chat.retry")}
          </button>
        </div>
      </div>
    );
  }
}
