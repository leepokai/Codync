// Relay WebSocket upgrades (§7.1) and public routine webhook deliveries (§7.8), both forwarded
// to the computer's Durable Object.

import { COMPUTER_ID, computerIdFor, fromB64url, verifySig, type SignedRequest } from "../auth";
import { HOOK_ID, HOOK_MAX_BODY } from "../hooks";
import type { Env } from "../index";
import { ApiError, type Ctx } from "./common";

function checkUpgrade(c: Ctx): void {
  if (c.url.searchParams.get("v") !== "1") throw new ApiError("upgradeRequired");
  if (c.req.headers.get("Upgrade")?.toLowerCase() !== "websocket") throw new ApiError("badRequest", "Expected a WebSocket upgrade");
}

/**
 * Whether the relay and public webhooks are only for computers in an account (docs/plans/optional-sign-in.md). A
 * switch so a release can turn it on once phones and hosts that follow the rule have shipped.
 */
export const relayNeedsAccount = (env: Env) => env.RELAY_NEEDS_ACCOUNT === "1";

/**
 * Whether a device key may open a socket on a computer's relay. In use: the computer is in an account and the device
 * is signed in to that account. Pairing: the device has an account, which the computer is then asked to join.
 */
export async function mayUseRelay(db: D1Database, owner: string | null, key: string, pair: boolean): Promise<boolean> {
  const { results } = await db
    .prepare("SELECT owner_user_id, signed_out_at FROM devices WHERE sign_pub = ? AND revoked_at IS NULL")
    .bind(key)
    .all<{ owner_user_id: string; signed_out_at: number | null }>();
  if (pair) return results.length > 0;
  return owner !== null && results.some((d) => d.owner_user_id === owner && d.signed_out_at === null);
}

/** Forwards the upgrade to the computer's DO in a request the Worker builds itself; the client's URL never reaches it. */
async function forward(c: Ctx, computerId: string, role: "host" | "device", s: SignedRequest, pair: string): Promise<Response> {
  const headers = new Headers({
    "X-Codync-Internal": "1",
    "X-Codync-Role": role,
    "X-Codync-Key": s.kid,
    "X-Codync-Nonce": s.nonce,
    "X-Codync-Ts": String(s.ts),
    "X-Codync-Pair": pair,
    "X-Codync-Computer": computerId,
  });
  for (const [name, value] of c.req.headers) {
    if (name === "upgrade" || name.startsWith("sec-websocket-")) headers.set(name, value);
  }
  const res = await c.env.RELAY.get(c.env.RELAY.idFromName(computerId)).fetch(new Request("https://do/relay", { headers }));
  if (res.status === 101) return res;
  const err = (await res.json().catch(() => null)) as { error?: { code?: string } } | null;
  throw new ApiError(err?.error?.code ?? "internal", undefined, res.status);
}

export async function relayHost(c: Ctx): Promise<Response> {
  checkUpgrade(c);
  const s = await verifySig(c.req, c.raw, c.now);
  if (!s) throw new ApiError("badSignature");
  const id = await computerIdFor(fromB64url(s.kid, 32)!);
  const row = await c.env.DB.prepare("SELECT status FROM computers WHERE id = ?").bind(id).first<{ status: string }>();
  if (!row) throw new ApiError("unknownComputer", "Register first (POST /v1/host/register)");
  if (row.status !== "active") throw new ApiError("forbidden");
  return forward(c, id, "host", s, "");
}

export async function relayDevice(c: Ctx, [computerId]: string[]): Promise<Response> {
  checkUpgrade(c);
  if (!COMPUTER_ID.test(computerId!)) throw new ApiError("badRequest", "Invalid computerId");
  const pair = c.url.searchParams.get("pair") ?? "";
  if (pair && !fromB64url(pair, 16)) throw new ApiError("badRequest", "Invalid pair");
  const s = await verifySig(c.req, c.raw, c.now);
  if (!s) throw new ApiError("badSignature");
  // Only registered computers get a DO; random IDs never create one.
  const row = await c.env.DB.prepare("SELECT status, owner_user_id FROM computers WHERE id = ?")
    .bind(computerId)
    .first<{ status: string; owner_user_id: string | null }>();
  if (!row || row.status !== "active") throw new ApiError("unknownComputer");
  if (relayNeedsAccount(c.env) && !(await mayUseRelay(c.env.DB, row.owner_user_id, s.kid, pair !== ""))) {
    throw new ApiError("needsAccount", "This computer and this device must be in the same Codync account to use the relay.");
  }
  return forward(c, computerId!, "device", s, pair);
}

// ---- routine webhooks (§7.8) ----

/**
 * `POST /v1/hooks/:computerId/:hookId`: a public delivery for one of the computer's routines. The
 * computer's DO checks the key and queues it for the host; the host checks again before it runs.
 */
export async function routineHook(c: Ctx, [computerId, hookId]: string[]): Promise<Response> {
  if (!COMPUTER_ID.test(computerId!) || !HOOK_ID.test(hookId!)) throw new ApiError("notFound");
  if (c.raw.length > HOOK_MAX_BODY) throw new ApiError("tooLarge", "Deliveries are limited to 64 KB");
  if (c.env.HOOK_LIMITER) {
    const ip = c.req.headers.get("CF-Connecting-IP") ?? "unknown";
    if (!(await c.env.HOOK_LIMITER.limit({ key: `${ip}|${computerId}/${hookId}` })).success) throw new ApiError("rateLimited");
  }
  // Only registered computers get a DO; random IDs never create one.
  const row = await c.env.DB.prepare("SELECT status, owner_user_id FROM computers WHERE id = ?")
    .bind(computerId)
    .first<{ status: string; owner_user_id: string | null }>();
  if (!row || row.status !== "active") throw new ApiError("notFound");
  if (relayNeedsAccount(c.env) && !row.owner_user_id) {
    throw new ApiError("needsAccount", "Public webhooks need this computer in a Codync account.");
  }
  const headers = new Headers(c.req.headers);
  headers.set("X-Codync-Internal", "1");
  headers.set("X-Codync-Hook", hookId!);
  return c.env.RELAY.get(c.env.RELAY.idFromName(computerId!)).fetch(
    new Request("https://do/internal/hook", { method: "POST", headers, body: c.raw }),
  );
}
