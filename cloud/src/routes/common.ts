// Shared pieces of the /v1 handlers: errors, request context, body validation, caller auth and
// the grant/relay helpers several route groups use.

import {
  computerIdFor,
  fromB64url,
  NONCE_TTL_MS,
  verifyClerk,
  verifySig,
  type SignedRequest,
} from "../auth";
import type { Env } from "../index";

const MAX_BODY = 64 * 1024;

// ---- errors ----

const STATUS: Record<string, number> = {
  badRequest: 400,
  unauthenticated: 401,
  badSignature: 401,
  forbidden: 403,
  accountDeleted: 403,
  notFound: 404,
  unknownComputer: 404,
  alreadyClaimed: 409,
  conflict: 409,
  claimExpired: 410,
  requestExpired: 410,
  upgradeRequired: 426,
  tooLarge: 413,
  rateLimited: 429,
  internal: 500,
  notConfigured: 503,
};

export class ApiError extends Error {
  readonly status: number;
  constructor(
    readonly code: string,
    message?: string,
    status?: number,
  ) {
    super(message ?? code);
    this.status = status ?? STATUS[code] ?? 500;
  }
}

// ---- request context ----

export interface Ctx {
  req: Request;
  env: Env;
  exec: ExecutionContext;
  url: URL;
  /** Raw body bytes: Codync-Sig hashes exactly these. */
  raw: Uint8Array;
  now: number;
}

type Body = Record<string, unknown>;

export function body(c: Ctx): Body {
  if (c.raw.length > MAX_BODY) throw new ApiError("badRequest", "Body too large");
  if (!c.raw.length) return {};
  let v: unknown;
  try {
    v = JSON.parse(new TextDecoder().decode(c.raw));
  } catch {
    throw new ApiError("badRequest", "Body is not JSON");
  }
  if (!v || typeof v !== "object" || Array.isArray(v)) throw new ApiError("badRequest", "Body must be an object");
  return v as Body;
}

export function str(b: Body, key: string, max = 100): string {
  const v = b[key];
  if (typeof v !== "string" || !v.trim() || v.length > max) throw new ApiError("badRequest", `Invalid ${key}`);
  return v;
}

export function optStr(b: Body, key: string, max = 100): string | null {
  return b[key] === undefined || b[key] === null ? null : str(b, key, max);
}

export function oneOf<T extends string>(b: Body, key: string, allowed: readonly T[]): T {
  const v = b[key];
  if (!allowed.includes(v as T)) throw new ApiError("badRequest", `Invalid ${key}`);
  return v as T;
}

/** A base64url field of exactly `len` bytes; returns the original string. */
export function key(b: Body, name: string, len: number): string {
  if (!fromB64url(b[name], len)) throw new ApiError("badRequest", `Invalid ${name}`);
  return b[name] as string;
}

// ---- auth ----

interface User {
  userId: string;
  email: string | null;
  createdAt: number;
}

export async function user(c: Ctx): Promise<User> {
  const clerk = await verifyClerk(c.req, c.env);
  if (!clerk) throw new ApiError("unauthenticated");
  const row = await c.env.DB.prepare(
    `INSERT INTO accounts(user_id, email, created_at) VALUES (?1, ?2, ?3)
     ON CONFLICT(user_id) DO UPDATE SET email = COALESCE(excluded.email, accounts.email)
     RETURNING status, email, created_at`,
  )
    .bind(clerk.userId, clerk.email, c.now)
    .first<{ status: string; email: string | null; created_at: number }>();
  if (!row || row.status === "deleted") throw new ApiError("accountDeleted");
  return { userId: clerk.userId, email: row.email, createdAt: row.created_at };
}

/** Codync-Sig with D1 replay protection (§5). */
export async function signed(c: Ctx): Promise<SignedRequest> {
  const s = await verifySig(c.req, c.raw, c.now);
  if (!s) throw new ApiError("badSignature");
  const r = await c.env.DB.prepare("INSERT OR IGNORE INTO sig_nonces(kid, nonce, expires_at) VALUES (?, ?, ?)")
    .bind(s.kid, s.nonce, c.now + NONCE_TTL_MS)
    .run();
  if (r.meta.changes !== 1) throw new ApiError("badSignature");
  return s;
}

export interface ComputerRow {
  id: string;
  sign_pub: string;
  box_pub: string;
  owner_user_id: string | null;
  name: string;
  platform: string;
  device: string | null;
  version: string;
  status: string;
  online: number;
  last_seen_at: number | null;
  claimed_at: number | null;
}

/** Sig(host): the signing key names the computer; it must be registered and not blocked. */
export async function host(c: Ctx): Promise<ComputerRow> {
  const s = await signed(c);
  const id = await computerIdFor(fromB64url(s.kid, 32)!);
  const row = await c.env.DB.prepare("SELECT * FROM computers WHERE id = ?").bind(id).first<ComputerRow>();
  if (!row) throw new ApiError("unknownComputer");
  if (row.status !== "active") throw new ApiError("forbidden");
  return row;
}

/** Sig(dev) for a device registered (and not revoked) under `userId`. */
export async function device(c: Ctx, userId: string): Promise<{ id: string; dk: string }> {
  const s = await signed(c);
  const row = await c.env.DB.prepare(
    "SELECT id FROM devices WHERE owner_user_id = ? AND sign_pub = ? AND revoked_at IS NULL",
  )
    .bind(userId, s.kid)
    .first<{ id: string }>();
  if (!row) throw new ApiError("notFound");
  return { id: row.id, dk: s.kid };
}

export async function ownedComputer(c: Ctx, id: string, userId: string): Promise<ComputerRow> {
  const row = await c.env.DB.prepare("SELECT * FROM computers WHERE id = ? AND owner_user_id = ?")
    .bind(id, userId)
    .first<ComputerRow>();
  if (!row) throw new ApiError("notFound");
  return row;
}

// ---- shared pieces ----

export function computerView(r: ComputerRow & { access?: string | null }) {
  return {
    computerId: r.id,
    name: r.name,
    platform: r.platform,
    device: r.device,
    signKey: r.sign_pub,
    boxKey: r.box_pub,
    version: r.version,
    online: r.online === 1,
    lastSeenAt: r.last_seen_at,
    claimedAt: r.claimed_at,
    access: r.access ?? null,
  };
}

export const audit = (env: Env, actor: string, action: string, target: string | null, result: string, now: number) =>
  env.DB.prepare("INSERT INTO audit_events(actor, action, target, result, created_at) VALUES (?, ?, ?, ?, ?)").bind(
    actor,
    action,
    target,
    result,
    now,
  );

interface Blocked {
  dk: string;
  grantId: string;
}

/** Statements revoking every active grant matching `filter`, and the DO blocks they imply, per computer. */
export async function revokeGrants(
  env: Env,
  filter: string,
  binds: unknown[],
  reason: string,
  now: number,
): Promise<{ stmts: D1PreparedStatement[]; blocks: Map<string, Blocked[]> }> {
  const { results } = await env.DB.prepare(
    `SELECT g.id, g.computer_id, d.sign_pub FROM grants g JOIN devices d ON d.id = g.device_id
     WHERE g.status = 'active' AND (${filter})`,
  )
    .bind(...binds)
    .all<{ id: string; computer_id: string; sign_pub: string }>();
  const blocks = new Map<string, Blocked[]>();
  const stmts = results.map((g) => {
    blocks.set(g.computer_id, [...(blocks.get(g.computer_id) ?? []), { dk: g.sign_pub, grantId: g.id }]);
    return env.DB.prepare(
      "UPDATE grants SET status = 'revoked', revoked_at = ?, revoked_reason = ? WHERE id = ? AND status = 'active'",
    ).bind(now, reason, g.id);
  });
  return { stmts, blocks };
}

/** Tells a computer's DO: block these devices (closing their sockets), then `cloud.changed` to the host. */
export async function notify(env: Env, computerId: string, blocked: Blocked[] = []): Promise<void> {
  const path = blocked.length ? "/internal/block" : "/internal/changed";
  try {
    const res = await env.RELAY.get(env.RELAY.idFromName(computerId)).fetch(`https://do${path}`, {
      method: "POST",
      headers: { "X-Codync-Internal": "1", "Content-Type": "application/json" },
      body: JSON.stringify(blocked.length ? { devices: blocked } : {}),
    });
    if (!res.ok) console.error("relay notify failed", { computerId, path, status: res.status });
  } catch (e) {
    console.error("relay notify failed", { computerId, path, error: String(e) });
  }
}

/** Removes a computer from its account: grants revoked, pending requests cancelled, sockets closed. */
export async function unclaim(env: Env, id: string, actor: string, now: number): Promise<void> {
  const { stmts, blocks } = await revokeGrants(env, "g.computer_id = ?", [id], "unclaimed", now);
  await env.DB.batch([
    ...stmts,
    env.DB.prepare("UPDATE computers SET owner_user_id = NULL, claimed_at = NULL, updated_at = ? WHERE id = ?").bind(now, id),
    env.DB.prepare(
      "UPDATE access_requests SET status = 'cancelled', decided_at = ? WHERE computer_id = ? AND status = 'pending'",
    ).bind(now, id),
    audit(env, actor, "computer.unclaim", id, "ok", now),
  ]);
  await notify(env, id, blocks.get(id));
}


export interface RequestRow {
  id: string;
  computer_id: string;
  device_id: string;
  status: string;
  host_nonce: string | null;
  device_nonce: string | null;
  expires_at: number;
  grant_id: string | null;
}

export const effectiveStatus = (r: RequestRow, now: number) => (r.status === "pending" && r.expires_at <= now ? "expired" : r.status);
