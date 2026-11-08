import type { Message } from "./types";

export interface DisplayTurn { key: string; runId: string | null; user?: Message; guidance: Message[]; assistants: Message[] }
export type DisplayItem = { kind: "turn"; turn: DisplayTurn } | { kind: "control"; message: Message };

/** Group model/tool iterations into one task; old messages without run IDs group by user turn. */
export function displayTurns(messages: Message[]): DisplayItem[] {
  const items: DisplayItem[] = [];
  let turn: DisplayTurn | undefined;
  const start = (m: Message) => {
    turn = { key: m.id, runId: m.run_id ?? null, guidance: [], assistants: [] };
    items.push({ kind: "turn", turn });
    return turn;
  };
  for (const m of messages) {
    if (m.role === "tool") continue;
    if (m.role === "control") { items.push({ kind: "control", message: m }); turn = undefined; continue; }
    if (m.role === "user") {
      if (m.run_id && turn?.runId === m.run_id) turn.guidance.push(m);
      else start(m).user = m;
    } else if (m.role === "assistant") {
      if (!turn || (m.run_id && turn.runId && m.run_id !== turn.runId)) start(m);
      if (turn!.runId === null && m.run_id) turn!.runId = m.run_id;
      turn!.assistants.push(m);
    }
  }
  return items;
}
