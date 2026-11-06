import type { Message } from "./types";

export interface RoundStats {
  /** 1-based number of the latest round (= user messages so far). */
  round: number;
  /** Model calls in the latest round (one per agent step). */
  steps: number;
  /** Sum of measured thinking time in the latest round. */
  thinkingMs: number;
  /** When the latest round started (its user message), for the live timer. */
  startedAt: number;
}

/** Stats for the latest round: everything after the last user message. */
export function roundStats(messages: Message[]): RoundStats | null {
  let lastUser = -1;
  let round = 0;
  messages.forEach((m, i) => {
    if (m.role === "user") {
      lastUser = i;
      round += 1;
    }
  });
  if (lastUser < 0) return null;
  let steps = 0;
  let thinkingMs = 0;
  for (const m of messages.slice(lastUser + 1)) {
    if (m.role !== "assistant" || m.status === "error") continue;
    steps += 1;
    thinkingMs += m.timing?.thinking_ms ?? 0;
  }
  return { round, steps, thinkingMs, startedAt: messages[lastUser].created_at };
}
