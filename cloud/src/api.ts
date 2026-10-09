// /v1 HTTP handlers (spec §8.4) and the relay upgrade routes (§7.1). Route groups live in
// `routes/`; this module is what the router and the cron call.

import type { Env } from "./index";
import type { Ctx } from "./routes/common";

export { ApiError, type Ctx } from "./routes/common";
export * from "./routes/account";
export * from "./routes/host";
export * from "./routes/relay";
export * from "./routes/webhooks";

export const VERSION = "2.2.0";

// ---- public ----

export const health = async () => ({ ok: true, version: VERSION });

/**
 * Where a connector's sign-in page returns when the user signed in on the phone. Stateless: it bounces
 * the query (code, state or error) to the app, which hands it to the host over the E2E channel. The code
 * is useless without the PKCE verifier that only the host holds.
 */
export const oauthCallback = async (c: Ctx) =>
  new Response(null, { status: 302, headers: { Location: `codync://oauth${c.url.search}`, "Cache-Control": "no-store" } });

/** Separate callback route keeps installed production clients and older dev builds working. */
export const oauthDevCallback = async (c: Ctx) =>
  new Response(null, { status: 302, headers: { Location: `codync-dev://oauth${c.url.search}`, "Cache-Control": "no-store" } });

// ---- cron (every 15 minutes) ----

export async function sweep(env: Env, now: number): Promise<void> {
  await env.DB.batch([
    env.DB.prepare("DELETE FROM sig_nonces WHERE expires_at <= ?").bind(now),
    env.DB.prepare("DELETE FROM claims WHERE expires_at <= ?").bind(now - 86_400_000),
    env.DB.prepare(
      `UPDATE access_requests SET status = 'expired', host_nonce = NULL, device_nonce = NULL
       WHERE status = 'pending' AND expires_at <= ?`,
    ).bind(now),
    env.DB.prepare("DELETE FROM audit_events WHERE created_at <= ?").bind(now - 30 * 86_400_000),
  ]);
}
