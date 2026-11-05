import type { Effort, ModelCaps, ProviderId, Settings } from "./types";

/** Parse "5.5" / "6" style version numbers out of a model id. */
function version(re: RegExp, id: string): number | null {
  const m = re.exec(id);
  if (!m) return null;
  return Number(m[1]) + (m[2] ? Number(`0.${m[2]}`) : 0);
}

/**
 * "Older generation" models hidden by Settings › Hide legacy models.
 * This only affects what the picker shows — never which parameters are sent.
 * - OpenAI: GPT below 5.5, mini/nano below 6, o-series, gpt-4o family
 * - Claude: generations below 5, or marked deprecated by the Models API
 * - DeepSeek: retired V2/V3/chat/reasoner/coder names and the retired v4-flash alias
 */
export function isLegacy(m: ModelCaps): boolean {
  const id = m.id.toLowerCase();
  if (m.provider === "openai") {
    if (/^o\d/.test(id) || id.includes("4o") || id.startsWith("chatgpt-")) return true;
    const v = version(/^gpt-(\d+)(?:\.(\d+))?/, id);
    if (v === null) return false;
    if (v < 5.5) return true;
    return v < 6 && /-(mini|nano)\b/.test(id);
  }
  if (m.provider === "anthropic") {
    if (m.note === "deprecated") return true;
    const v = version(/^claude-(?:opus|sonnet|haiku|fable|mythos)-(\d+)(?:-(\d{1,2})(?!\d))?/, id) ?? version(/^claude-(\d+)(?:-(\d))?/, id);
    return v !== null && v < 5;
  }
  if (m.provider === "deepseek") {
    return /^deepseek-(chat|reasoner|coder|v2|v3|v4-flash)/.test(id);
  }
  return false;
}

export const modelKey = (p: ProviderId, id: string) => `${p}:${id}`;

export function isHidden(m: ModelCaps, s: Pick<Settings, "hide_legacy_models" | "hidden_models">): boolean {
  return s.hidden_models.includes(modelKey(m.provider, m.id)) || (s.hide_legacy_models && isLegacy(m));
}

/** Provider-specific description of what the reasoning control maps to. */
export const EFFORT_PARAM: Record<ProviderId, string> = {
  openai: "reasoning.effort",
  deepseek: "thinking + reasoning_effort",
  anthropic: "output_config.effort",
};

/** Value actually sent for a level (DeepSeek "Off" = thinking disabled). */
export function effortWire(p: ProviderId, e: Effort): string {
  if (p === "deepseek" && e === "none") return "thinking: disabled";
  return e;
}
