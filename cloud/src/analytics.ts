// Codync Cloud's own usage analytics (docs/features/analytics.md): what the Worker does for a cloud computer (its
// wakes), sent to PostHog as `$lib: codync-cloud`. Only for a machine whose own host last said its owner shares
// analytics (`cloud_machines.analytics`, sent when the switch changes and with each sleep request), and only where
// `POSTHOG_HOST` is set (tests and local runs leave it out). Feature use only: counts and listed values, never ids, names, keys or error messages.

import { sha256, utf8 } from "./auth";
import type { CloudMachineRow, WakeCause, WokenOutcome } from "./cloud-machines";
import type { Env } from "./index";

/** The host's write-only project token (`host/src/analytics.rs`): safe to ship. */
const TOKEN = "phc_rQHvxdSFKnMquPfMjmiMhXhM4tqJ5qCYsWuVEoFxCCWX";
const TIMEOUT_MS = 5_000;

/** What the Worker reports, with its own properties (docs/reference/analytics-events.md). */
export type CloudEvent =
  | { event: "cloud_computer_woken"; outcome: WokenOutcome; cause: WakeCause }
  | { event: "cloud_computer_awake"; seconds: number; attempts: number };

type Machine = Pick<CloudMachineRow, "analytics" | "user_id" | "provider" | "computer_id">;

/** Sends `e` about machine `m` to its owner's account (as its host's own events go), at `now`. Never throws: a
 * failure is logged, and a send gives up after `TIMEOUT_MS`. */
export async function capture(env: Env, m: Machine, e: CloudEvent, now: number): Promise<void> {
  if (m.analytics !== 1 || !env.POSTHOG_HOST) return;
  const { event, ...properties } = e;
  try {
    const batch = [
      {
        event,
        distinct_id: m.user_id,
        timestamp: new Date(now).toISOString(),
        uuid: crypto.randomUUID(),
        properties: {
          ...properties,
          $lib: "codync-cloud",
          env: env.ANALYTICS_ENV === "dev" ? "dev" : "main",
          provider: m.provider,
          ...(m.computer_id && { computer: await computerTag(m.computer_id) }),
          $process_person_profile: true,
          // The sender is Cloudflare, not the user.
          $geoip_disable: true,
        },
      },
    ];
    const res = await fetch(new URL("/batch/", env.POSTHOG_HOST), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ api_key: TOKEN, batch }),
      signal: AbortSignal.timeout(TIMEOUT_MS),
    });
    if (!res.ok) console.error(JSON.stringify({ msg: "analytics batch refused", event, status: res.status }));
  } catch (e) {
    console.error(JSON.stringify({ msg: "analytics batch not sent", event, error: String(e) }));
  }
}

/** A computer's analytics key, as its host computes it (`computer_tag`): the first 8 bytes of SHA-256 of
 * `codync-analytics:<id>`, in hex, so the Worker's events join the host's without the id. */
export async function computerTag(computerId: string): Promise<string> {
  const hash = await sha256(utf8(`codync-analytics:${computerId}`));
  return [...hash.subarray(0, 8)].map((b) => b.toString(16).padStart(2, "0")).join("");
}
