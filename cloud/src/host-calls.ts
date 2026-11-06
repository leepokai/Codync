// Calls the relay DO puts to its host and waits on, by id: the cloud assistant's tool calls (§7.9).
// One is open only while the request that asked is, so none keeps a sleeping DO awake.

import { randomId } from "./auth";

export interface Answer {
  ok: boolean;
  result?: unknown;
  error?: string;
  /** No answer: the host is gone, or didn't answer in time (an older host ignores the call). */
  code?: "offline" | "timeout";
}

const MAX_ERROR = 2000;

export class HostCalls {
  private readonly open = new Map<string, (a: Answer) => void>();

  ask(host: WebSocket, msg: Record<string, unknown>, ms: number): Promise<Answer> {
    const id = randomId("", 12);
    return new Promise((resolve) => {
      const timer = setTimeout(() => this.settle(id, { ok: false, code: "timeout" }), ms);
      this.open.set(id, (a) => {
        clearTimeout(timer);
        resolve(a);
      });
      try {
        host.send(JSON.stringify({ ...msg, id }));
      } catch {
        this.settle(id, { ok: false, code: "offline" });
      }
    });
  }

  /** `{"t":"assistant.result","id","ok","result"|"error"}` from the host. */
  answered(msg: Record<string, unknown>): void {
    if (typeof msg.id !== "string") return;
    const error = typeof msg.error === "string" ? msg.error.slice(0, MAX_ERROR) : "Refused";
    this.settle(msg.id, msg.ok === true ? { ok: true, result: msg.result } : { ok: false, error });
  }

  /** The host went away: nothing it was asked gets an answer. */
  offline(): void {
    for (const id of [...this.open.keys()]) this.settle(id, { ok: false, code: "offline" });
  }

  private settle(id: string, a: Answer): void {
    const done = this.open.get(id);
    this.open.delete(id);
    done?.(a);
  }
}
