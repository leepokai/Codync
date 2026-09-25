// Codync cloud: accounts (Clerk), computers, access grants (D1) and the end-to-end encrypted relay
// (one ComputerRelay Durable Object per computer). The cloud only ever sees ciphertext and routing
// metadata; the host stays the sole authority on which devices may decrypt (docs/remote-relay-spec.md).

import * as api from "./api";
import { ApiError, type Ctx } from "./api";
import type { ComputerRelay } from "./relay";

export { ComputerRelay } from "./relay";

export interface Env {
  DB: D1Database;
  RELAY: DurableObjectNamespace<ComputerRelay>;
  /** Workers Rate Limiting binding for new computer registrations. */
  REGISTER_LIMITER?: RateLimit;
  CLERK_ISSUER: string;
  CLERK_SECRET_KEY?: string;
  /** dev / e2e only: networkless verification with this public key. */
  CLERK_JWT_KEY?: string;
  CLERK_AUTHORIZED_PARTIES?: string;
  CLERK_WEBHOOK_SECRET?: string;
}

type Handler = (c: Ctx, params: string[]) => Promise<unknown>;

const ID = "([A-Za-z0-9_-]{1,64})";
const routes: [string, RegExp, Handler][] = [
  ["GET", /^\/v1\/health$/, api.health],
  ["GET", /^\/v1\/me$/, api.me],
  ["POST", /^\/v1\/devices$/, api.registerDevice],
  ["GET", /^\/v1\/devices$/, api.listDevices],
  ["DELETE", new RegExp(`^/v1/devices/${ID}$`), api.revokeDevice],
  ["POST", /^\/v1\/claims$/, api.createClaim],
  ["POST", new RegExp(`^/v1/claims/${ID}/complete$`), api.completeClaim],
  ["GET", /^\/v1\/computers$/, api.listComputers],
  ["PATCH", new RegExp(`^/v1/computers/${ID}$`), api.renameComputer],
  ["DELETE", new RegExp(`^/v1/computers/${ID}$`), api.removeComputer],
  ["POST", new RegExp(`^/v1/computers/${ID}/access-requests$`), api.createAccessRequest],
  ["GET", new RegExp(`^/v1/computers/${ID}/grants$`), api.listGrants],
  ["DELETE", new RegExp(`^/v1/computers/${ID}/grants/${ID}$`), api.revokeGrant],
  ["GET", new RegExp(`^/v1/access-requests/${ID}$`), api.getAccessRequest],
  ["POST", new RegExp(`^/v1/access-requests/${ID}/reveal$`), api.revealAccessRequest],
  ["DELETE", new RegExp(`^/v1/access-requests/${ID}$`), api.cancelAccessRequest],
  ["POST", /^\/v1\/host\/register$/, api.hostRegister],
  ["GET", /^\/v1\/host\/state$/, api.hostState],
  ["POST", new RegExp(`^/v1/host/access-requests/${ID}/nonce$`), api.hostNonce],
  ["POST", new RegExp(`^/v1/host/access-requests/${ID}/decision$`), api.hostDecision],
  ["POST", new RegExp(`^/v1/host/grants/${ID}/revoke$`), api.hostRevokeGrant],
  ["POST", /^\/v1\/host\/unclaim$/, api.hostUnclaim],
  ["GET", /^\/v1\/relay\/host$/, api.relayHost],
  // Any segment reaches the handler so a malformed computerId gets its 400 (§7.1).
  ["GET", /^\/v1\/relay\/device\/([^/]+)$/, api.relayDevice],
  ["POST", /^\/v1\/webhooks\/clerk$/, api.clerkWebhook],
];

export default {
  async fetch(req: Request, env: Env, exec: ExecutionContext): Promise<Response> {
    const requestId = req.headers.get("cf-ray") ?? crypto.randomUUID();
    try {
      const url = new URL(req.url);
      for (const [method, pattern, handler] of routes) {
        const m = pattern.exec(url.pathname);
        if (!m || req.method !== method) continue;
        const raw = new Uint8Array(await req.arrayBuffer());
        const out = await handler({ req, env, exec, url, raw, now: Date.now() }, m.slice(1));
        return out instanceof Response ? out : Response.json(out);
      }
      throw new ApiError("notFound");
    } catch (e) {
      const err = e instanceof ApiError ? e : new ApiError("internal");
      if (!(e instanceof ApiError)) console.error("unhandled", { requestId, error: e instanceof Error ? e.stack : String(e) });
      return Response.json({ error: { code: err.code, message: err.message }, requestId }, { status: err.status });
    }
  },

  async scheduled(_controller: ScheduledController, env: Env): Promise<void> {
    await api.sweep(env, Date.now());
  },
} satisfies ExportedHandler<Env>;
