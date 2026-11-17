// Clerk-authenticated account routes: devices, claims, computers, access requests and grants.

import { parseApp, recordApp } from "../app-versions";
import { computerIdFor, fromB64url, randomId, utf8, verifyEd25519 } from "../auth";
import { CERT_TTL_MS, certify } from "../certificate";
import { machineFor, newClaim, type CloudMachineRow } from "../cloud-machines";
import { cloudView } from "./cloud-computers";
import {
  ApiError,
  audit,
  body,
  computerView,
  device,
  effectiveStatus,
  key,
  notify,
  notifyAwake,
  oneOf,
  optStr,
  ownedComputer,
  revokeGrants,
  signed,
  str,
  unclaim,
  user,
  type Body,
  type ComputerRow,
  type Ctx,
  type RequestRow,
} from "./common";

const CLAIM_TTL_MS = 5 * 60_000;
const REQUEST_TTL_MS = 10 * 60_000;
const REQUESTS_PER_HOUR = 20;

export const claimCanonical = (claimId: string, nonce: string, userId: string, computerId: string, boxKey: string) =>
  ["codync/claim/v1", claimId, nonce, userId, computerId, boxKey].join("\n");

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
  const platform = oneOf(b, "platform", ["ios", "macos", "linux", "windows", "android"] as const);
  const existing = await c.env.DB.prepare("SELECT * FROM devices WHERE owner_user_id = ? AND sign_pub = ?")
    .bind(u.userId, s.kid)
    .first<DeviceRow>();
  if (existing?.revoked_at) throw new ApiError("forbidden", "This device key was revoked; create a new one");
  if (existing) {
    // Signing in again: its computers may push to it again.
    await c.env.DB.prepare("UPDATE devices SET name = ?, platform = ?, last_used_at = ?, signed_out_at = NULL WHERE id = ?")
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

/** A certificate that this (registered, unrevoked) device key is in the caller's account, for pairing in person. */
export async function deviceCertificate(c: Ctx) {
  const u = await user(c);
  const s = await signed(c);
  if (!c.env.DEVICE_CERT_KEY) throw new ApiError("notConfigured", "This cloud doesn't issue device certificates");
  const known = await c.env.DB.prepare("SELECT 1 FROM devices WHERE owner_user_id = ? AND sign_pub = ? AND revoked_at IS NULL")
    .bind(u.userId, s.kid)
    .first();
  if (!known) throw new ApiError("forbidden", "Register this device with the account first");
  const certificate = await certify(c.env.DEVICE_CERT_KEY, {
    v: 1,
    deviceKey: s.kid,
    userId: u.userId,
    email: u.email,
    exp: c.now + CERT_TTL_MS,
  });
  return { certificate };
}

/** Signing out of a phone: it keeps its key and access, but the account's computers drop its push tickets on their
 * next state pull, told to pull now. Registering the device again (signing back in) undoes it. */
export async function signOutDevice(c: Ctx) {
  const u = await user(c);
  const d = await device(c, u.userId);
  await c.env.DB.prepare("UPDATE devices SET signed_out_at = ? WHERE id = ?").bind(c.now, d.id).run();
  const { results } = await c.env.DB.prepare("SELECT id FROM computers WHERE owner_user_id = ?")
    .bind(u.userId)
    .all<{ id: string }>();
  await Promise.all(results.map((r) => notify(c.env, r.id)));
  return {};
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
  const expiresAt = c.now + CLAIM_TTL_MS;
  return { ...(await newClaim(c.env, u.userId, expiresAt)), expiresAt };
}

export async function completeClaim(c: Ctx, [claimId]: string[]) {
  const u = await user(c);
  return finishClaim(c, u.userId, claimId!, body(c));
}

/** Checks a claim the computer signed and puts the computer in `userId`'s account: from the
 * signed-in desktop app, or from the computer itself with a ticket (`hostClaim`). */
export async function finishClaim(c: Ctx, userId: string, claimId: string, b: Body) {
  const signKey = key(b, "signKey", 32);
  const boxKey = key(b, "boxKey", 32);
  const sig = fromB64url(b.sig, 64);
  const computerId = str(b, "computerId", 22);
  const name = str(b, "name");
  const platform = oneOf(b, "platform", ["macos", "linux", "windows"] as const);
  const device = optStr(b, "device", 32);
  const version = str(b, "version", 32);
  if (!sig) throw new ApiError("badRequest", "Invalid sig");
  if (computerId !== (await computerIdFor(fromB64url(signKey, 32)!))) throw new ApiError("badRequest", "computerId doesn't match signKey");
  const claim = await c.env.DB.prepare("SELECT nonce FROM claims WHERE id = ? AND user_id = ?")
    .bind(claimId, userId)
    .first<{ nonce: string }>();
  if (!claim) throw new ApiError("notFound");
  const canonical = claimCanonical(claimId, claim.nonce, userId, computerId, boxKey);
  if (!(await verifyEd25519(fromB64url(signKey, 32)!, sig, utf8(canonical)))) throw new ApiError("badSignature");
  const existing = await c.env.DB.prepare("SELECT status FROM computers WHERE id = ?").bind(computerId).first<{ status: string }>();
  if (existing && existing.status !== "active") throw new ApiError("forbidden");

  const t = c.now;
  const [consumed, , owned] = await c.env.DB.batch([
    c.env.DB.prepare(
      `UPDATE claims SET consumed_at = ?1, computer_id = ?2
       WHERE id = ?3 AND user_id = ?4 AND consumed_at IS NULL AND expires_at > ?1`,
    ).bind(t, computerId, claimId, userId),
    // box_pub always comes from the signed boxKey; the row is created only for a valid claim.
    c.env.DB.prepare(
      `INSERT INTO computers(id, sign_pub, box_pub, name, platform, device, version, created_at, updated_at)
       SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8 WHERE EXISTS(SELECT 1 FROM claims WHERE id = ?9 AND consumed_at = ?8)
       ON CONFLICT(id) DO UPDATE SET box_pub = excluded.box_pub, updated_at = excluded.updated_at`,
    ).bind(computerId, signKey, boxKey, name, platform, device, version, t, claimId),
    c.env.DB.prepare(
      `UPDATE computers SET owner_user_id = ?1, claimed_at = ?2, updated_at = ?2
       WHERE id = ?3 AND owner_user_id IS NULL AND EXISTS(SELECT 1 FROM claims WHERE id = ?4 AND consumed_at = ?2)`,
    ).bind(userId, t, computerId, claimId),
  ]);
  if (consumed!.meta.changes === 0) throw new ApiError("claimExpired");
  const row = await c.env.DB.prepare("SELECT * FROM computers WHERE id = ?").bind(computerId).first<ComputerRow>();
  if (owned!.meta.changes === 0 && row?.owner_user_id !== userId) {
    await audit(c.env, userId, "computer.claim", computerId, "alreadyClaimed", t).run();
    throw new ApiError("alreadyClaimed");
  }
  await audit(c.env, userId, "computer.claim", computerId, "ok", t).run();
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
    await recordApp(c.env, s.kid, parseApp(c.req.headers.get("Codync-App")), c.now);
  }
  const { results } = await c.env.DB.prepare(
    `SELECT c.*, CASE
       WHEN ?1 IS NULL THEN NULL
       WHEN EXISTS(SELECT 1 FROM grants g WHERE g.computer_id = c.id AND g.device_id = ?1 AND g.status = 'active') THEN 'granted'
       WHEN EXISTS(SELECT 1 FROM access_requests r WHERE r.computer_id = c.id AND r.device_id = ?1
                   AND r.status = 'pending' AND r.expires_at > ?3) THEN 'pending'
       ELSE 'none' END AS access, m.provider AS cloud_provider, m.state AS cloud_state
     FROM computers c LEFT JOIN cloud_machines m ON m.computer_id = c.id AND m.state != 'deleted'
     WHERE c.owner_user_id = ?2 ORDER BY c.claimed_at`,
  )
    .bind(deviceId, u.userId, c.now)
    .all<ComputerRow & { access: string | null; cloud_provider: CloudMachineRow["provider"] | null; cloud_state: CloudMachineRow["state"] | null }>();
  return {
    computers: results.map((r) => ({
      ...computerView(r),
      // Codync Cloud: which provider runs it and whether it sleeps; `null` for the user's own computers.
      cloud: r.cloud_provider && r.cloud_state ? cloudView({ provider: r.cloud_provider, state: r.cloud_state }) : null,
    })),
  };
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
  // Out of the account, a cloud computer would keep running with nobody to wake, use or delete it.
  if (await machineFor(c.env, id!)) throw new ApiError("conflict", "Delete the cloud computer in Codync Cloud instead.");
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
  await notifyAwake(c.env, computerId!);
  return { requestId, expiresAt };
}

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
