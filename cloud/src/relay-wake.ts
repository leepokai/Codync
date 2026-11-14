// A sleeping Codync Cloud computer's wakes as its relay keeps them, in its meta so they outlive the DO's hibernation:
// when the alarm wakes it next and why (`WakeSchedule`), and each wake from the first try until its host is ready again
// (`WakeEpisode`). Both Worker events about wakes come from here (`analytics.ts`): `cloud_computer_woken` for each start
// asked of the provider, `cloud_computer_awake` once its host is back.

import { capture } from "./analytics";
import { machineFor, type Wake, type WakeCause } from "./cloud-machines";
import type { Env } from "./index";

/** The relay's meta table. */
export interface Meta {
  get(k: string): string | null;
  set(k: string, v: string): void;
}

const CAUSES: readonly string[] = ["device", "message", "access", "routine", "retry"] satisfies WakeCause[];

/** The alarm's next wake: when (`wakeAt`, ms) and why (`wakeCause`), always written together. */
export class WakeSchedule {
  constructor(private readonly meta: Meta) {}

  /** The next wake at `at` for `cause`; none for `null`. */
  set(at: number | null, cause: WakeCause): void {
    this.meta.set("wakeAt", at === null ? "" : String(at));
    if (at !== null) this.meta.set("wakeCause", cause);
  }

  /** The next wake at `at` for `cause`, unless one is already due by then; true when this one was set. */
  sooner(at: number, cause: WakeCause): boolean {
    const due = this.pending();
    if (due && due <= at) return false;
    this.set(at, cause);
    return true;
  }

  /** When the next wake is due (ms), 0 for none. */
  pending(): number {
    return Number(this.meta.get("wakeAt") || 0);
  }

  /** Takes off a wake due by `now`, with its cause; null when none is. One set before causes were kept reads as
   * `routine`, what such a wake most likely was. */
  due(now: number): WakeCause | null {
    const at = this.pending();
    if (!at || at > now) return null;
    this.clear();
    const cause = this.meta.get("wakeCause") ?? "";
    return CAUSES.includes(cause) ? (cause as WakeCause) : "routine";
  }

  clear(): void {
    this.meta.set("wakeAt", "");
  }
}

/** Reports what a wake for `cause` asked of the provider came to (`cloud_computer_woken`), at `now`. */
export const reportWoken = (env: Env, woke: NonNullable<Wake["woken"]>, cause: WakeCause, now: number) =>
  capture(env, woke.machine, { event: "cloud_computer_woken", outcome: woke.outcome, cause }, now);

/** When the episode's first wake was tried (ms), and how many starts it asked of the provider since. */
const START = "wakeStart";
const STARTS = "wakeStarts";

/** A wake from its first try until the host is ready again, reported then as `cloud_computer_awake`: how long it took
 * and how many starts it asked of the provider. */
export class WakeEpisode {
  constructor(private readonly meta: Meta) {}

  /** A wake tried at `now` while the host is away: the first starts the episode. One that can't wake the computer ends
   * it: nothing retries, and a computer that isn't a cloud computer never has one. */
  tried(w: Wake, now: number): void {
    if (w.result === "none") return this.end();
    if (!this.meta.get(START)) this.meta.set(START, String(now));
    if (w.woken) this.meta.set(STARTS, String(this.starts() + 1));
  }

  /** The host of `computerId` is ready at `now`: ends the episode, reported when the provider was asked to start the
   * machine in it (a host that reconnects on its own, or that never stopped, has nothing to report). */
  async ready(env: Env, computerId: string, now: number): Promise<void> {
    const start = Number(this.meta.get(START) || 0);
    const starts = this.starts();
    this.end();
    if (!start || !starts) return;
    try {
      const m = await machineFor(env, computerId);
      if (m) await capture(env, m, { event: "cloud_computer_awake", seconds: Math.round((now - start) / 100) / 10, attempts: starts }, now);
    } catch (e) {
      console.error(JSON.stringify({ msg: "cloud computer awake report failed", error: String(e) }));
    }
  }

  private starts(): number {
    return Number(this.meta.get(STARTS) || 0);
  }

  /** Forgets the episode (written only when there is one: every host's ready reads it). */
  private end(): void {
    if (!this.meta.get(START) && !this.meta.get(STARTS)) return;
    this.meta.set(START, "");
    this.meta.set(STARTS, "");
  }
}
