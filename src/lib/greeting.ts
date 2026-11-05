import { translate, type Key, type Lang } from "./i18n";

/** Time-of-day bucket for "Good morning/afternoon/evening". */
export function timeOfDay(hour: number): "morning" | "noon" | "afternoon" | "evening" | "night" {
  if (hour >= 5 && hour < 11) return "morning";
  if (hour >= 11 && hour < 13) return "noon";
  if (hour >= 13 && hour < 18) return "afternoon";
  if (hour >= 18 && hour < 23) return "evening";
  return "night";
}

const folderName = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;

/**
 * Welcome-screen greeting, picked at random from the variants that apply:
 * "<time> , <name>" (when a name is known), "What should we do in <folder>?"
 * (when a workspace is set) and "What can I help you build?".
 */
export function pickGreeting(opts: { lang: Lang; name: string; workspace: string | null; now: Date; random: number }): string {
  const t = (k: Key, v?: Record<string, string>) => translate(opts.lang, k, v);
  const options: string[] = [t("welcome.title")];
  const name = opts.name.trim();
  if (name) options.push(t(`greeting.${timeOfDay(opts.now.getHours())}` as Key, { name }));
  if (opts.workspace) options.push(t("greeting.folder", { folder: folderName(opts.workspace) }));
  const i = Math.min(options.length - 1, Math.floor(opts.random * options.length));
  return options[i];
}
