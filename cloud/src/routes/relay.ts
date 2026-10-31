// Relay WebSocket upgrades (§7.1) and public routine webhook deliveries (§7.8), both forwarded
// to the computer's Durable Object.

import { compareVersions, parseApp, recordApp } from "../app-versions";
import { COMPUTER_ID, computerIdFor, fromB64url, verifySig, type SignedRequest } from "../auth";
import { HOOK_ID, HOOK_MAX_BODY } from "../hooks";
import { ApiError, notify, type Ctx } from "./common";

function checkUpgrade(c: Ctx): void {
  if (c.url.searchParams.get("v") !== "1") throw new ApiError("upgradeRequired");
  if (c.req.headers.get("Upgrade")?.toLowerCase() !== "websocket") throw new ApiError("badRequest", "Expected a WebSocket upgrade");
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
  const row = await c.env.DB.prepare("SELECT status, version FROM computers WHERE id = ?")
    .bind(computerId)
    .first<{ status: string; version: string }>();
  if (!row || row.status !== "active") throw new ApiError("unknownComputer");
  const app = parseApp(c.req.headers.get("Codync-App"));
  await recordApp(c.env, s.kid, app, c.now);
  // An app that needs a newer host: a cloud computer reads its update order now (a sleeping one on waking).
  if (app?.minHost && compareVersions(app.minHost, row.version) > 0) c.exec.waitUntil(notify(c.env, computerId!));
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
  const row = await c.env.DB.prepare("SELECT status FROM computers WHERE id = ?").bind(computerId).first<{ status: string }>();
  if (!row || row.status !== "active") throw new ApiError("notFound");
  const headers = new Headers(c.req.headers);
  headers.set("X-Codync-Internal", "1");
  headers.set("X-Codync-Hook", hookId!);
  return c.env.RELAY.get(c.env.RELAY.idFromName(computerId!)).fetch(
    new Request("https://do/internal/hook", { method: "POST", headers, body: c.raw }),
  );
}
