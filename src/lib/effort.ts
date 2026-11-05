import type { Effort, Mode, ModelCaps, ProviderId } from "./types";
import { EFFORT_ORDER } from "./types";

/** Effort levels the model really accepts, in ascending order. */
export function supportedEfforts(caps: ModelCaps | undefined): Effort[] {
  if (!caps) return [];
  return EFFORT_ORDER.filter((e) => caps.efforts.includes(e));
}

/** Same rule as the backend's `clamp_effort`: never return an unsupported level. */
export function clampEffort(caps: ModelCaps | undefined, wanted: Effort | null | undefined): Effort | null {
  const levels = supportedEfforts(caps);
  if (!caps || levels.length === 0) return null;
  if (wanted && levels.includes(wanted)) return wanted;
  if (caps.default_effort && levels.includes(caps.default_effort)) return caps.default_effort;
  return levels[0];
}

export function effortKey(provider: ProviderId, model: string) {
  return `${provider}:${model}`;
}

export function findCaps(models: ModelCaps[], provider: ProviderId, id: string | undefined) {
  return models.find((m) => m.provider === provider && m.id === id);
}

/** Model used when the user hasn't picked one for this provider. */
export function defaultModel(models: ModelCaps[], provider: ProviderId, mode: Mode): string | undefined {
  const own = models.filter((m) => m.provider === provider);
  return (own.find((m) => m.recommended.includes(mode) && m.available !== false) ?? own.find((m) => m.available !== false) ?? own[0])?.id;
}
