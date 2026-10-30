// Codync cloud: accounts (Clerk), computers, access grants (D1) and the end-to-end encrypted relay
// (one ComputerRelay Durable Object per computer). The cloud only ever sees ciphertext and routing
// metadata; the host stays the sole authority on which devices may decrypt (docs/reference/remote-relay.md).

import * as api from "./api";
import { ApiError, type Ctx } from "./api";
import type { ComputerRelay } from "./relay";
import { nudgeReleaseGate } from "./release-gate";
import { icon, MAX_ICON_BYTES } from "./routes/icons";
import { readBody } from "./request-body";

export { ComputerRelay } from "./relay";

export interface Env {
  ICONS?: R2Bucket;
  ICON_LIMITER?: RateLimit;
  DB: D1Database;
  RELAY: DurableObjectNamespace<ComputerRelay>;
  /** Workers Rate Limiting binding for new computer registrations. */
  REGISTER_LIMITER?: RateLimit;
  /** Server-only Cloudflare Realtime TURN key and credential-issuing token. */
  TURN_KEY_ID?: string;
  TURN_KEY_API_TOKEN?: string;
  TURN_LIMITER?: RateLimit;
  /** Public routine webhook deliveries per sender IP and hook. */
  HOOK_LIMITER?: RateLimit;
  /** Cloud assistant (MCP connector) requests per account. */
  ASSISTANT_LIMITER?: RateLimit;
  CLERK_ISSUER: string;
  CLERK_SECRET_KEY?: string;
  /** dev / e2e only: networkless verification with this public key. */
  CLERK_JWT_KEY?: string;
  CLERK_AUTHORIZED_PARTIES?: string;
  CLERK_WEBHOOK_SECRET?: string;
  /** GitHub token (Actions: write) that wakes the release gate; unset outside production. */
  RELEASE_GATE_TOKEN?: string;
}

type Handler = (c: Ctx, params: string[]) => Promise<unknown>;

/** Above every route's own limit (64 KB bodies); the routes check theirs. */
const MAX_REQUEST = 128 * 1024;

/** wrangler.toml's crons, matched exactly: one that matches neither throws. */
const SWEEP_CRON = "*/15 * * * *";
const RELEASE_GATE_CRON = "*/5 * * * *";

const ID = "([A-Za-z0-9_-]{1,64})";
const routes: [string, RegExp, Handler][] = [
  ["PUT", /^\/v1\/host\/icons\/([^/]+)$/, icon],
  ["GET", /^\/v1\/host\/icons\/([^/]+)$/, icon],
  ["DELETE", /^\/v1\/host\/icons\/([^/]+)$/, icon],
  ["GET", /^\/v1\/health$/, api.health],
  ["GET", /^\/v1\/oauth\/callback$/, api.oauthCallback],
  ["GET", /^\/v1\/oauth\/callback\/dev$/, api.oauthDevCallback],
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
  ["POST", /^\/v1\/host\/screen-ice$/, api.hostScreenIce],
  ["POST", new RegExp(`^/v1/host/access-requests/${ID}/nonce$`), api.hostNonce],
  ["POST", new RegExp(`^/v1/host/access-requests/${ID}/decision$`), api.hostDecision],
  ["POST", new RegExp(`^/v1/host/grants/${ID}/revoke$`), api.hostRevokeGrant],
  ["POST", /^\/v1\/host\/unclaim$/, api.hostUnclaim],
  ["GET", /^\/v1\/relay\/host$/, api.relayHost],
  // Any segment reaches the handler so a malformed computerId gets its 400 (§7.1).
  ["GET", /^\/v1\/relay\/device\/([^/]+)$/, api.relayDevice],
  ["POST", /^\/v1\/webhooks\/clerk$/, api.clerkWebhook],
  ["POST", new RegExp(`^/v1/hooks/${ID}/${ID}$`), api.routineHook],
  // The cloud assistant's MCP connector (mcp.codync.dev, §7.9).
  ["POST", /^\/mcp$/, api.mcp],
  ["GET", /^\/mcp$/, api.mcpStream],
  ["GET", /^\/\.well-known\/oauth-protected-resource(?:\/mcp)?$/, api.resourceMetadata],
  ["GET", /^\/\.well-known\/oauth-authorization-server$/, api.authServerMetadata],
  ["OPTIONS", /^\/(?:mcp|\.well-known\/oauth-[a-z-]+(?:\/mcp)?)$/, api.assistantPreflight],
];

export default {
  async fetch(req: Request, env: Env, exec: ExecutionContext): Promise<Response> {
    const requestId = req.headers.get("cf-ray") ?? crypto.randomUUID();
    try {
      const url = new URL(req.url);
      for (const [method, pattern, handler] of routes) {
        const m = pattern.exec(url.pathname);
        if (!m || req.method !== method) continue;
        // Every route takes small bodies; refuse a big one before reading it.
        const raw = await readBody(req, handler === icon ? MAX_ICON_BYTES : MAX_REQUEST);
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

  async scheduled(controller: ScheduledController, env: Env): Promise<void> {
    if (controller.cron === RELEASE_GATE_CRON) await nudgeReleaseGate(env);
    else if (controller.cron === SWEEP_CRON) await api.sweep(env, Date.now());
    else throw new Error(`unknown cron ${controller.cron}`);
  },
} satisfies ExportedHandler<Env>;
