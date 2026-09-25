// /v1 HTTP handlers (spec §8.4) and the relay upgrade routes (§7.1).

import { Webhook } from "svix";
import {
  COMPUTER_ID,
  computerIdFor,
  fromB64url,
  NONCE_TTL_MS,
  randomId,
  utf8,
  verifyClerk,
  verifyEd25519,
  verifySig,
  type SignedRequest,
} from "./auth";
import type { Env } from "./index";

export const VERSION = "3.0.0";

const CLAIM_TTL_MS = 5 * 60_000;
const REQUEST_TTL_MS = 10 * 60_000;
const REQUESTS_PER_HOUR = 20;
const MAX_BODY = 64 * 1024;
const SCOPES = '["control","screen"]';

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
  rateLimited: 429,
  internal: 500,
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

function body(c: Ctx): Body {
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

function str(b: Body, key: string, max = 100): string {
  const v = b[key];
  if (typeof v !== "string" || !v.trim() || v.length > max) throw new ApiError("badRequest", `Invalid ${key}`);
  return v;
}

function optStr(b: Body, key: string, max = 100): string | null {
  return b[key] === undefined || b[key] === null ? null : str(b, key, max);
}

function oneOf<T extends string>(b: Body, key: string, allowed: readonly T[]): T {
  const v = b[key];
  if (!allowed.includes(v as T)) throw new ApiError("badRequest", `Invalid ${key}`);
  return v as T;
}

/** A base64url field of exactly `len` bytes; returns the original string. */
function key(b: Body, name: string, len: number): string {
  if (!fromB64url(b[name], len)) throw new ApiError("badRequest", `Invalid ${name}`);
  return b[name] as string;
}

// ---- auth ----

interface User {
  userId: string;
  email: string | null;
  createdAt: number;
}

async function user(c: Ctx): Promise<User> {
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
async function signed(c: Ctx): Promise<SignedRequest> {
  const s = await verifySig(c.req, c.raw, c.now);
  if (!s) throw new ApiError("badSignature");
  const r = await c.env.DB.prepare("INSERT OR IGNORE INTO sig_nonces(kid, nonce, expires_at) VALUES (?, ?, ?)")
    .bind(s.kid, s.nonce, c.now + NONCE_TTL_MS)
    .run();
  if (r.meta.changes !== 1) throw new ApiError("badSignature");
  return s;
}

interface ComputerRow {
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
async function host(c: Ctx): Promise<ComputerRow> {
  const s = await signed(c);
  const id = await computerIdFor(fromB64url(s.kid, 32)!);
  const row = await c.env.DB.prepare("SELECT * FROM computers WHERE id = ?").bind(id).first<ComputerRow>();
  if (!row) throw new ApiError("unknownComputer");
  if (row.status !== "active") throw new ApiError("forbidden");
  return row;
}

/** Sig(dev) for a device registered (and not revoked) under `userId`. */
async function device(c: Ctx, userId: string): Promise<{ id: string; dk: string }> {
  const s = await signed(c);
  const row = await c.env.DB.prepare(
    "SELECT id FROM devices WHERE owner_user_id = ? AND sign_pub = ? AND revoked_at IS NULL",
  )
    .bind(userId, s.kid)
    .first<{ id: string }>();
  if (!row) throw new ApiError("notFound");
  return { id: row.id, dk: s.kid };
}

async function ownedComputer(c: Ctx, id: string, userId: string): Promise<ComputerRow> {
  const row = await c.env.DB.prepare("SELECT * FROM computers WHERE id = ? AND owner_user_id = ?")
    .bind(id, userId)
    .first<ComputerRow>();
  if (!row) throw new ApiError("notFound");
  return row;
}

// ---- shared pieces ----

function computerView(r: ComputerRow & { access?: string | null }) {
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

const audit = (env: Env, actor: string, action: string, target: string | null, result: string, now: number) =>
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
async function revokeGrants(
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
async function notify(env: Env, computerId: string, blocked: Blocked[] = []): Promise<void> {
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
async function unclaim(env: Env, id: string, actor: string, now: number): Promise<void> {
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

export const claimCanonical = (claimId: string, nonce: string, userId: string, computerId: string, boxKey: string) =>
  ["codync/claim/v1", claimId, nonce, userId, computerId, boxKey].join("\n");

// ---- public ----

export const health = async () => ({ ok: true, version: VERSION });

// ---- Clerk (users) ----

export async function me(c: Ctx) {
  const u = await user(c);
  return { userId: u.userId, email: u.email, createdAt: u.createdAt };
}

interface DeviceRow {
  id: string;
  sign_pub: string;
  name: string;
  platform: string;
  created_at: number;
  last_used_at: number | null;
  revoked_at: number | null;
}

const deviceView = (d: DeviceRow) => ({
  deviceId: d.id,
  deviceKey: d.sign_pub,
  name: d.name,
  platform: d.platform,
  createdAt: d.created_at,
});

export async function registerDevice(c: Ctx) {
  const u = await user(c);
  const s = await signed(c);
  const b = body(c);
  const name = str(b, "name");
  const platform = oneOf(b, "platform", ["ios", "macos"] as const);
  const existing = await c.env.DB.prepare("SELECT * FROM devices WHERE owner_user_id = ? AND sign_pub = ?")
    .bind(u.userId, s.kid)
    .first<DeviceRow>();
  if (existing?.revoked_at) throw new ApiError("forbidden", "This device key was revoked; create a new one");
  if (existing) {
    await c.env.DB.prepare("UPDATE devices SET name = ?, platform = ?, last_used_at = ? WHERE id = ?")
      .bind(name, platform, c.now, existing.id)
      .run();
    return { device: deviceView({ ...existing, name, platform }) };
  }
  const row: DeviceRow = {
    id: randomId("dev_"),
    sign_pub: s.kid,
    name,
    platform,
    created_at: c.now,
    last_used_at: c.now,
    revoked_at: null,
  };
  await c.env.DB.prepare(
    `INSERT INTO devices(id, owner_user_id, sign_pub, name, platform, created_at, last_used_at) VALUES (?, ?, ?, ?, ?, ?, ?)
     ON CONFLICT(owner_user_id, sign_pub) DO NOTHING`,
  )
    .bind(row.id, u.userId, row.sign_pub, name, platform, c.now, c.now)
    .run();
  // A concurrent registration of the same key wins the insert; report whichever row exists.
  const saved = await c.env.DB.prepare("SELECT * FROM devices WHERE owner_user_id = ? AND sign_pub = ?")
    .bind(u.userId, s.kid)
    .first<DeviceRow>();
  return { device: deviceView(saved ?? row) };
}

export async function listDevices(c: Ctx) {
  const u = await user(c);
  const { results } = await c.env.DB.prepare("SELECT * FROM devices WHERE owner_user_id = ? ORDER BY created_at")
    .bind(u.userId)
    .all<DeviceRow>();
  return {
    devices: results.map((d) => ({ ...deviceView(d), lastUsedAt: d.last_used_at, revoked: d.revoked_at !== null })),
  };
}

export async function revokeDevice(c: Ctx, [deviceId]: string[]) {
  const u = await user(c);
  const d = await c.env.DB.prepare("SELECT id, revoked_at FROM devices WHERE id = ? AND owner_user_id = ?")
    .bind(deviceId, u.userId)
    .first<{ id: string; revoked_at: number | null }>();
  if (!d) throw new ApiError("notFound");
  if (d.revoked_at) return {};
  const { stmts, blocks } = await revokeGrants(c.env, "g.device_id = ?", [deviceId], "deviceRevoked", c.now);
  const pending = await c.env.DB.prepare(
    "SELECT DISTINCT computer_id FROM access_requests WHERE device_id = ? AND status = 'pending'",
  )
    .bind(deviceId)
    .all<{ computer_id: string }>();
  await c.env.DB.batch([
    ...stmts,
    c.env.DB.prepare("UPDATE devices SET revoked_at = ? WHERE id = ?").bind(c.now, deviceId),
    c.env.DB.prepare(
      "UPDATE access_requests SET status = 'cancelled', decided_at = ? WHERE device_id = ? AND status = 'pending'",
    ).bind(c.now, deviceId),
    audit(c.env, u.userId, "device.revoke", deviceId, "ok", c.now),
  ]);
  const computers = new Set([...blocks.keys(), ...pending.results.map((r) => r.computer_id)]);
  await Promise.all([...computers].map((id) => notify(c.env, id, blocks.get(id))));
  return {};
}

export async function createClaim(c: Ctx) {
  const u = await user(c);
  const claimId = randomId("clm_");
  const nonce = randomId();
  const expiresAt = c.now + CLAIM_TTL_MS;
  await c.env.DB.prepare("INSERT INTO claims(id, user_id, nonce, expires_at) VALUES (?, ?, ?, ?)")
    .bind(claimId, u.userId, nonce, expiresAt)
    .run();
  return { claimId, nonce, expiresAt };
}

export async function completeClaim(c: Ctx, [claimId]: string[]) {
  const u = await user(c);
  const b = body(c);
  const signKey = key(b, "signKey", 32);
  const boxKey = key(b, "boxKey", 32);
  const sig = fromB64url(b.sig, 64);
  const computerId = str(b, "computerId", 22);
  const name = str(b, "name");
  const platform = oneOf(b, "platform", ["macos", "linux"] as const);
  const device = optStr(b, "device", 32);
  const version = str(b, "version", 32);
  if (!sig) throw new ApiError("badRequest", "Invalid sig");
  if (computerId !== (await computerIdFor(fromB64url(signKey, 32)!))) throw new ApiError("badRequest", "computerId doesn't match signKey");
  const claim = await c.env.DB.prepare("SELECT nonce FROM claims WHERE id = ? AND user_id = ?")
    .bind(claimId, u.userId)
    .first<{ nonce: string }>();
  if (!claim) throw new ApiError("notFound");
  const canonical = claimCanonical(claimId, claim.nonce, u.userId, computerId, boxKey);
  if (!(await verifyEd25519(fromB64url(signKey, 32)!, sig, utf8(canonical)))) throw new ApiError("badSignature");
  const existing = await c.env.DB.prepare("SELECT status FROM computers WHERE id = ?").bind(computerId).first<{ status: string }>();
  if (existing && existing.status !== "active") throw new ApiError("forbidden");

  const t = c.now;
  const [consumed, , owned] = await c.env.DB.batch([
    c.env.DB.prepare(
      `UPDATE claims SET consumed_at = ?1, computer_id = ?2
       WHERE id = ?3 AND user_id = ?4 AND consumed_at IS NULL AND expires_at > ?1`,
    ).bind(t, computerId, claimId, u.userId),
    // box_pub always comes from the signed boxKey; the row is created only for a valid claim.
    c.env.DB.prepare(
      `INSERT INTO computers(id, sign_pub, box_pub, name, platform, device, version, created_at, updated_at)
       SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8 WHERE EXISTS(SELECT 1 FROM claims WHERE id = ?9 AND consumed_at = ?8)
       ON CONFLICT(id) DO UPDATE SET box_pub = excluded.box_pub, updated_at = excluded.updated_at`,
    ).bind(computerId, signKey, boxKey, name, platform, device, version, t, claimId),
    c.env.DB.prepare(
      `UPDATE computers SET owner_user_id = ?1, claimed_at = ?2, updated_at = ?2
       WHERE id = ?3 AND owner_user_id IS NULL AND EXISTS(SELECT 1 FROM claims WHERE id = ?4 AND consumed_at = ?2)`,
    ).bind(u.userId, t, computerId, claimId),
  ]);
  if (consumed!.meta.changes === 0) throw new ApiError("claimExpired");
  const row = await c.env.DB.prepare("SELECT * FROM computers WHERE id = ?").bind(computerId).first<ComputerRow>();
  if (owned!.meta.changes === 0 && row?.owner_user_id !== u.userId) {
    await audit(c.env, u.userId, "computer.claim", computerId, "alreadyClaimed", t).run();
    throw new ApiError("alreadyClaimed");
  }
  await audit(c.env, u.userId, "computer.claim", computerId, "ok", t).run();
  await notify(c.env, computerId);
  return { computer: computerView(row!) };
}

export async function listComputers(c: Ctx) {
  const u = await user(c);
  let deviceId: string | null = null;
  if (c.req.headers.has("Codync-Sig")) {
    const s = await signed(c);
    const d = await c.env.DB.prepare(
      "UPDATE devices SET last_used_at = ? WHERE owner_user_id = ? AND sign_pub = ? AND revoked_at IS NULL RETURNING id",
    )
      .bind(c.now, u.userId, s.kid)
      .first<{ id: string }>();
    deviceId = d?.id ?? null;
  }
  const { results } = await c.env.DB.prepare(
    `SELECT c.*, CASE
       WHEN ?1 IS NULL THEN NULL
       WHEN EXISTS(SELECT 1 FROM grants g WHERE g.computer_id = c.id AND g.device_id = ?1 AND g.status = 'active') THEN 'granted'
       WHEN EXISTS(SELECT 1 FROM access_requests r WHERE r.computer_id = c.id AND r.device_id = ?1
                   AND r.status = 'pending' AND r.expires_at > ?3) THEN 'pending'
       ELSE 'none' END AS access
     FROM computers c WHERE c.owner_user_id = ?2 ORDER BY c.claimed_at`,
  )
    .bind(deviceId, u.userId, c.now)
    .all<ComputerRow & { access: string | null }>();
  return { computers: results.map(computerView) };
}

export async function renameComputer(c: Ctx, [id]: string[]) {
  const u = await user(c);
  const name = str(body(c), "name");
  await ownedComputer(c, id!, u.userId);
  await c.env.DB.prepare("UPDATE computers SET name = ?, updated_at = ? WHERE id = ?").bind(name, c.now, id).run();
  return { computer: computerView(await ownedComputer(c, id!, u.userId)) };
}

export async function removeComputer(c: Ctx, [id]: string[]) {
  const u = await user(c);
  await ownedComputer(c, id!, u.userId);
  await unclaim(c.env, id!, u.userId, c.now);
  return {};
}

export async function createAccessRequest(c: Ctx, [computerId]: string[]) {
  const u = await user(c);
  const dev = await device(c, u.userId);
  const commit = key(body(c), "commit", 32);
  await ownedComputer(c, computerId!, u.userId);
  const recent = await c.env.DB.prepare("SELECT COUNT(*) AS n FROM access_requests WHERE user_id = ? AND created_at > ?")
    .bind(u.userId, c.now - 3600_000)
    .first<{ n: number }>();
  if ((recent?.n ?? 0) >= REQUESTS_PER_HOUR) throw new ApiError("rateLimited");
  const granted = await c.env.DB.prepare(
    "SELECT 1 FROM grants WHERE computer_id = ? AND device_id = ? AND status = 'active'",
  )
    .bind(computerId, dev.id)
    .first();
  if (granted) throw new ApiError("conflict", "This device already has access");
  const requestId = randomId("req_");
  const expiresAt = c.now + REQUEST_TTL_MS;
  // A commit is never reused: an earlier pending request is cancelled, not refreshed.
  await c.env.DB.batch([
    c.env.DB.prepare(
      "UPDATE access_requests SET status = 'cancelled', decided_at = ? WHERE computer_id = ? AND device_id = ? AND status = 'pending'",
    ).bind(c.now, computerId, dev.id),
    c.env.DB.prepare(
      `INSERT INTO access_requests(id, computer_id, device_id, user_id, status, commit_hash, created_at, expires_at)
       VALUES (?, ?, ?, ?, 'pending', ?, ?, ?)`,
    ).bind(requestId, computerId, dev.id, u.userId, commit, c.now, expiresAt),
  ]);
  await notify(c.env, computerId!);
  return { requestId, expiresAt };
}

interface RequestRow {
  id: string;
  computer_id: string;
  device_id: string;
  status: string;
  host_nonce: string | null;
  device_nonce: string | null;
  expires_at: number;
  grant_id: string | null;
}

const effectiveStatus = (r: RequestRow, now: number) => (r.status === "pending" && r.expires_at <= now ? "expired" : r.status);

export async function getAccessRequest(c: Ctx, [id]: string[]) {
  const u = await user(c);
  const r = await c.env.DB.prepare("SELECT * FROM access_requests WHERE id = ? AND user_id = ?")
    .bind(id, u.userId)
    .first<RequestRow>();
  if (!r) throw new ApiError("notFound");
  return {
    requestId: r.id,
    computerId: r.computer_id,
    status: effectiveStatus(r, c.now),
    expiresAt: r.expires_at,
    ...(r.host_nonce ? { hostNonce: r.host_nonce } : {}),
    ...(r.grant_id && r.status === "approved" ? { grantId: r.grant_id } : {}),
  };
}

export async function revealAccessRequest(c: Ctx, [id]: string[]) {
  const u = await user(c);
  const dev = await device(c, u.userId);
  const nonce = key(body(c), "nonce", 32);
  const r = await c.env.DB.prepare(
    `UPDATE access_requests SET device_nonce = ?1
     WHERE id = ?2 AND user_id = ?3 AND device_id = ?4 AND status = 'pending' AND expires_at > ?5
       AND host_nonce IS NOT NULL AND device_nonce IS NULL`,
  )
    .bind(nonce, id, u.userId, dev.id, c.now)
    .run();
  if (r.meta.changes === 0) {
    const row = await c.env.DB.prepare("SELECT * FROM access_requests WHERE id = ? AND user_id = ? AND device_id = ?")
      .bind(id, u.userId, dev.id)
      .first<RequestRow>();
    if (!row) throw new ApiError("notFound");
    if (effectiveStatus(row, c.now) === "expired") throw new ApiError("requestExpired");
    throw new ApiError("conflict", row.host_nonce ? "Already revealed" : "The computer hasn't answered yet");
  }
  const row = await c.env.DB.prepare("SELECT computer_id FROM access_requests WHERE id = ?").bind(id).first<{ computer_id: string }>();
  await notify(c.env, row!.computer_id);
  return {};
}

export async function cancelAccessRequest(c: Ctx, [id]: string[]) {
  const u = await user(c);
  const row = await c.env.DB.prepare("SELECT * FROM access_requests WHERE id = ? AND user_id = ?")
    .bind(id, u.userId)
    .first<RequestRow>();
  if (!row) throw new ApiError("notFound");
  const r = await c.env.DB.prepare(
    "UPDATE access_requests SET status = 'cancelled', decided_at = ? WHERE id = ? AND status = 'pending'",
  )
    .bind(c.now, id)
    .run();
  if (r.meta.changes) await notify(c.env, row.computer_id);
  return {};
}

export async function listGrants(c: Ctx, [id]: string[]) {
  const u = await user(c);
  await ownedComputer(c, id!, u.userId);
  const { results } = await c.env.DB.prepare(
    `SELECT g.id, g.device_id, d.name, d.platform, g.scopes, g.created_at FROM grants g JOIN devices d ON d.id = g.device_id
     WHERE g.computer_id = ? AND g.status = 'active' ORDER BY g.created_at`,
  )
    .bind(id)
    .all<{ id: string; device_id: string; name: string; platform: string; scopes: string; created_at: number }>();
  return {
    grants: results.map((g) => ({
      grantId: g.id,
      deviceId: g.device_id,
      deviceName: g.name,
      platform: g.platform,
      scopes: JSON.parse(g.scopes) as string[],
      createdAt: g.created_at,
    })),
  };
}

export async function revokeGrant(c: Ctx, [id, grantId]: string[]) {
  const u = await user(c);
  await ownedComputer(c, id!, u.userId);
  const exists = await c.env.DB.prepare("SELECT 1 FROM grants WHERE id = ? AND computer_id = ?").bind(grantId, id).first();
  if (!exists) throw new ApiError("notFound");
  const { stmts, blocks } = await revokeGrants(c.env, "g.id = ? AND g.computer_id = ?", [grantId, id], "owner", c.now);
  if (!stmts.length) return {};
  await c.env.DB.batch([...stmts, audit(c.env, u.userId, "grant.revoke", grantId!, "ok", c.now)]);
  await notify(c.env, id!, blocks.get(id!));
  return {};
}

// ---- Sig(host) ----

export async function hostRegister(c: Ctx) {
  const s = await signed(c);
  const b = body(c);
  const boxKey = key(b, "boxKey", 32);
  const name = str(b, "name");
  const platform = oneOf(b, "platform", ["macos", "linux"] as const);
  const device = optStr(b, "device", 32);
  const version = str(b, "version", 32);
  const id = await computerIdFor(fromB64url(s.kid, 32)!);
  const existing = await c.env.DB.prepare("SELECT status, owner_user_id FROM computers WHERE id = ?")
    .bind(id)
    .first<{ status: string; owner_user_id: string | null }>();
  if (existing && existing.status !== "active") throw new ApiError("forbidden");
  if (!existing && c.env.REGISTER_LIMITER) {
    const ip = c.req.headers.get("CF-Connecting-IP") ?? "unknown";
    if (!(await c.env.REGISTER_LIMITER.limit({ key: ip })).success) throw new ApiError("rateLimited");
  }
  await c.env.DB.prepare(
    `INSERT INTO computers(id, sign_pub, box_pub, name, platform, device, version, created_at, updated_at)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
     ON CONFLICT(id) DO UPDATE SET name = excluded.name, device = excluded.device, version = excluded.version,
       box_pub = excluded.box_pub, updated_at = excluded.updated_at`,
  )
    .bind(id, s.kid, boxKey, name, platform, device, version, c.now)
    .run();
  return { computerId: id, owned: !!existing?.owner_user_id };
}

export async function hostState(c: Ctx) {
  const comp = await host(c);
  const owner = comp.owner_user_id
    ? await c.env.DB.prepare("SELECT user_id, email FROM accounts WHERE user_id = ? AND status = 'active'")
        .bind(comp.owner_user_id)
        .first<{ user_id: string; email: string | null }>()
    : null;
  const grants = await c.env.DB.prepare(
    `SELECT g.id, d.sign_pub, d.name, d.platform, g.scopes FROM grants g
     JOIN devices d ON d.id = g.device_id JOIN computers c ON c.id = g.computer_id
     WHERE g.computer_id = ? AND g.status = 'active' AND d.revoked_at IS NULL AND d.owner_user_id = c.owner_user_id
     ORDER BY g.created_at`,
  )
    .bind(comp.id)
    .all<{ id: string; sign_pub: string; name: string; platform: string; scopes: string }>();
  const requests = await c.env.DB.prepare(
    `SELECT r.id, d.sign_pub, d.name, d.platform, a.email, r.commit_hash, r.host_nonce, r.device_nonce, r.created_at, r.expires_at
     FROM access_requests r JOIN devices d ON d.id = r.device_id JOIN accounts a ON a.user_id = r.user_id
     JOIN computers c ON c.id = r.computer_id
     WHERE r.computer_id = ? AND r.status = 'pending' AND r.expires_at > ? AND d.revoked_at IS NULL
       AND r.user_id = c.owner_user_id
     ORDER BY r.created_at`,
  )
    .bind(comp.id, c.now)
    .all<{
      id: string;
      sign_pub: string;
      name: string;
      platform: string;
      email: string | null;
      commit_hash: string;
      host_nonce: string | null;
      device_nonce: string | null;
      created_at: number;
      expires_at: number;
    }>();
  return {
    owner: owner ? { userId: owner.user_id, email: owner.email } : null,
    grants: grants.results.map((g) => ({
      grantId: g.id,
      deviceKey: g.sign_pub,
      deviceName: g.name,
      platform: g.platform,
      scopes: JSON.parse(g.scopes) as string[],
    })),
    requests: requests.results.map((r) => ({
      requestId: r.id,
      deviceKey: r.sign_pub,
      deviceName: r.name,
      platform: r.platform,
      email: r.email,
      commit: r.commit_hash,
      ...(r.host_nonce ? { hostNonce: r.host_nonce } : {}),
      ...(r.device_nonce ? { deviceNonce: r.device_nonce } : {}),
      createdAt: r.created_at,
      expiresAt: r.expires_at,
    })),
  };
}

async function hostRequest(c: Ctx, computerId: string, id: string): Promise<RequestRow> {
  const r = await c.env.DB.prepare("SELECT * FROM access_requests WHERE id = ? AND computer_id = ?")
    .bind(id, computerId)
    .first<RequestRow>();
  if (!r) throw new ApiError("notFound");
  return r;
}

export async function hostNonce(c: Ctx, [id]: string[]) {
  const comp = await host(c);
  const nonce = key(body(c), "nonce", 32);
  const r = await c.env.DB.prepare(
    `UPDATE access_requests SET host_nonce = ?
     WHERE id = ? AND computer_id = ? AND status = 'pending' AND expires_at > ? AND host_nonce IS NULL`,
  )
    .bind(nonce, id, comp.id, c.now)
    .run();
  if (r.meta.changes === 0) {
    const row = await hostRequest(c, comp.id, id!);
    if (effectiveStatus(row, c.now) === "expired") throw new ApiError("requestExpired");
    throw new ApiError("conflict");
  }
  return {};
}

export async function hostDecision(c: Ctx, [id]: string[]) {
  const comp = await host(c);
  const decision = oneOf(body(c), "decision", ["approve", "deny"] as const);
  const row = await hostRequest(c, comp.id, id!);
  if (effectiveStatus(row, c.now) === "expired") throw new ApiError("requestExpired");
  if (decision === "deny") {
    const r = await c.env.DB.prepare(
      "UPDATE access_requests SET status = 'denied', decided_at = ? WHERE id = ? AND status = 'pending'",
    )
      .bind(c.now, id)
      .run();
    if (r.meta.changes === 0 && row.status !== "denied") throw new ApiError("conflict");
    await audit(c.env, `computer:${comp.id}`, "access.deny", id!, "ok", c.now).run();
    return { status: "denied" };
  }
  const g = randomId("grt_");
  // The device must still be live and belong to the computer's owner at the moment of approval.
  const [approved, inserted] = await c.env.DB.batch([
    c.env.DB.prepare(
      `UPDATE access_requests SET status = 'approved', decided_at = ?1, grant_id = ?2
       WHERE id = ?3 AND computer_id = ?4 AND status = 'pending' AND expires_at > ?1 AND device_nonce IS NOT NULL
         AND EXISTS(SELECT 1 FROM devices d JOIN computers c ON c.id = ?4
                    WHERE d.id = access_requests.device_id AND d.revoked_at IS NULL AND d.owner_user_id = c.owner_user_id)`,
    ).bind(c.now, g, id, comp.id),
    c.env.DB.prepare(
      `INSERT INTO grants(id, computer_id, device_id, scopes, status, created_at)
       SELECT ?1, computer_id, device_id, ?2, 'active', ?3 FROM access_requests WHERE id = ?4 AND grant_id = ?1
       ON CONFLICT DO NOTHING`,
    ).bind(g, SCOPES, c.now, id),
  ]);
  if (approved!.meta.changes === 0) {
    const latest = await hostRequest(c, comp.id, id!);
    // A double-click: the first approval already stands.
    if (latest.status === "approved") return { status: "approved", grantId: latest.grant_id };
    if (effectiveStatus(latest, c.now) === "expired") throw new ApiError("requestExpired");
    throw new ApiError("conflict", latest.device_nonce ? "The request can't be approved" : "The device hasn't revealed its code yet");
  }
  let grantId = g;
  if (inserted!.meta.changes === 0) {
    // grants_active: this device already holds an active grant on this computer; hand that one back.
    const existing = await c.env.DB.prepare(
      "SELECT id FROM grants WHERE computer_id = ? AND device_id = ? AND status = 'active'",
    )
      .bind(comp.id, row.device_id)
      .first<{ id: string }>();
    grantId = existing!.id;
    await c.env.DB.prepare("UPDATE access_requests SET grant_id = ? WHERE id = ?").bind(grantId, id).run();
  }
  await audit(c.env, `computer:${comp.id}`, "access.approve", id!, grantId, c.now).run();
  return { status: "approved", grantId };
}

export async function hostRevokeGrant(c: Ctx, [grantId]: string[]) {
  const comp = await host(c);
  const exists = await c.env.DB.prepare("SELECT 1 FROM grants WHERE id = ? AND computer_id = ?").bind(grantId, comp.id).first();
  if (!exists) throw new ApiError("notFound");
  const { stmts } = await revokeGrants(c.env, "g.id = ? AND g.computer_id = ?", [grantId, comp.id], "owner", c.now);
  if (stmts.length) await c.env.DB.batch([...stmts, audit(c.env, `computer:${comp.id}`, "grant.revoke", grantId!, "ok", c.now)]);
  return {};
}

export async function hostUnclaim(c: Ctx) {
  const comp = await host(c);
  if (comp.owner_user_id) await unclaim(c.env, comp.id, `computer:${comp.id}`, c.now);
  return {};
}

// ---- relay upgrades (§7.1) ----

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
  const row = await c.env.DB.prepare("SELECT status FROM computers WHERE id = ?").bind(computerId).first<{ status: string }>();
  if (!row || row.status !== "active") throw new ApiError("unknownComputer");
  return forward(c, computerId!, "device", s, pair);
}

// ---- webhooks ----

export async function clerkWebhook(c: Ctx) {
  if (!c.env.CLERK_WEBHOOK_SECRET) throw new ApiError("internal", "Webhook secret not configured");
  const payload = new TextDecoder().decode(c.raw);
  const h = (name: string) => c.req.headers.get(name) ?? "";
  let event: { type?: string; data?: { id?: unknown } } | null;
  try {
    // Throws unless the signature and timestamp check out; returns nothing.
    new Webhook(c.env.CLERK_WEBHOOK_SECRET).verify(payload, {
      "svix-id": h("svix-id"),
      "svix-timestamp": h("svix-timestamp"),
      "svix-signature": h("svix-signature"),
    });
    event = JSON.parse(payload);
  } catch {
    throw new ApiError("badSignature");
  }
  const seen = await c.env.DB.prepare("SELECT 1 FROM processed_events WHERE provider = 'clerk' AND event_id = ?")
    .bind(h("svix-id"))
    .first();
  if (seen) return {};
  if (event?.type === "user.deleted" && typeof event.data?.id === "string") await deleteAccount(c.env, event.data.id, c.now);
  await c.env.DB.prepare("INSERT OR IGNORE INTO processed_events(provider, event_id, processed_at) VALUES ('clerk', ?, ?)")
    .bind(h("svix-id"), c.now)
    .run();
  return {};
}

async function deleteAccount(env: Env, userId: string, now: number): Promise<void> {
  const { stmts, blocks } = await revokeGrants(
    env,
    "d.owner_user_id = ?1 OR g.computer_id IN (SELECT id FROM computers WHERE owner_user_id = ?1)",
    [userId],
    "accountDeleted",
    now,
  );
  const owned = await env.DB.prepare("SELECT id FROM computers WHERE owner_user_id = ?").bind(userId).all<{ id: string }>();
  await env.DB.batch([
    ...stmts,
    env.DB.prepare(
      `INSERT INTO accounts(user_id, status, created_at, deleted_at) VALUES (?1, 'deleted', ?2, ?2)
       ON CONFLICT(user_id) DO UPDATE SET status = 'deleted', deleted_at = ?2`,
    ).bind(userId, now),
    env.DB.prepare("UPDATE devices SET revoked_at = ? WHERE owner_user_id = ? AND revoked_at IS NULL").bind(now, userId),
    env.DB.prepare(
      `UPDATE access_requests SET status = 'cancelled', decided_at = ?1
       WHERE status = 'pending' AND (user_id = ?2 OR computer_id IN (SELECT id FROM computers WHERE owner_user_id = ?2))`,
    ).bind(now, userId),
    env.DB.prepare("UPDATE computers SET owner_user_id = NULL, claimed_at = NULL, updated_at = ? WHERE owner_user_id = ?").bind(
      now,
      userId,
    ),
    audit(env, "system", "account.delete", userId, "ok", now),
  ]);
  const computers = new Set([...blocks.keys(), ...owned.results.map((r) => r.id)]);
  await Promise.all([...computers].map((id) => notify(env, id, blocks.get(id))));
}

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
