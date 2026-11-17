// Sig(host) routes: a computer registers, reads its account state and decides access requests.

import { checkHostFloor, urgentUpdate } from "../app-versions";
import { b64url, computerIdFor, fromB64url, randomId, sha256 } from "../auth";
import { machineFor, nameComputer } from "../cloud-machines";
import { finishClaim } from "./account";
import {
  ApiError,
  audit,
  body,
  effectiveStatus,
  host,
  key,
  oneOf,
  optStr,
  revokeGrants,
  signed,
  str,
  unclaim,
  type Ctx,
  type RequestRow,
} from "./common";

const SCOPES = '["control","screen"]';

export async function hostRegister(c: Ctx) {
  const s = await signed(c);
  const b = body(c);
  const boxKey = key(b, "boxKey", 32);
  const name = str(b, "name");
  const platform = oneOf(b, "platform", ["macos", "linux", "windows"] as const);
  const device = optStr(b, "device", 32);
  const version = str(b, "version", 32);
  const id = await computerIdFor(fromB64url(s.kid, 32)!);
  const existing = await c.env.DB.prepare("SELECT status, owner_user_id FROM computers WHERE id = ?")
    .bind(id)
    .first<{ status: string; owner_user_id: string | null }>();
  if (existing && existing.status !== "active") throw new ApiError("forbidden");
  // A cloud computer below an emergency floor still registers: its update order arrives in its state. Its name is the
  // one its user gave it, whatever its host says.
  const machine = await machineFor(c.env, id);
  if (!machine) checkHostFloor(c.env, version);
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
    .bind(id, s.kid, boxKey, machine?.name ?? name, platform, device, version, c.now)
    .run();
  return { computerId: id, owned: !!existing?.owner_user_id };
}

/** A cloud computer Codync started for a signed-in user claims itself with the one-time ticket it
 * was given at creation (`/v1/cloud-computers`). The claim must be signed by the calling computer. */
export async function hostClaim(c: Ctx) {
  const s = await signed(c);
  const b = body(c);
  const claimId = str(b, "claimId", 64);
  if (b.signKey !== s.kid) throw new ApiError("badSignature");
  const claim = await c.env.DB.prepare("SELECT user_id FROM claims WHERE id = ?").bind(claimId).first<{ user_id: string }>();
  if (!claim) throw new ApiError("notFound");
  // The machine's keeper, which alone gets its vault key (`keeperVaultKey`): named once, by the claim from its first
  // boot, before any agent ran there.
  const keeper = b.keeperKey === undefined ? null : key(b, "keeperKey", 32);
  const done = await finishClaim(c, claim.user_id, claimId, b);
  const id = done.computer.computerId;
  const machine = await c.env.DB.prepare(
    "UPDATE cloud_machines SET computer_id = ?1, keeper_key = COALESCE(keeper_key, ?3) WHERE claim_id = ?2 AND state != 'deleted' RETURNING name",
  )
    .bind(id, claimId, keeper)
    .first<{ name: string }>();
  if (!machine) return done;
  // Its computer takes the name its user gave the machine.
  await nameComputer(c.env, id, machine.name, c.now).run();
  return { ...done, computer: { ...done.computer, name: machine.name } };
}

/** The host attests that this device passed its local screen authorization (including QR pairing).
 * Requiring a claimed, active account prevents anonymous host registration from issuing paid TURN.
 */
export async function hostScreenIce(c: Ctx): Promise<Response> {
  const comp = await host(c);
  const dk = key(body(c), "deviceKey", 32);
  const owner = comp.owner_user_id && await c.env.DB.prepare(
    "SELECT user_id FROM accounts WHERE user_id = ? AND status = 'active'",
  ).bind(comp.owner_user_id).first();
  if (!owner) throw new ApiError("forbidden", "Sign in on the computer to use remote screen relay.");
  if (!c.env.TURN_KEY_ID || !c.env.TURN_KEY_API_TOKEN || !c.env.TURN_LIMITER) {
    throw new ApiError("screenRelayUnavailable", "Remote screen relay is not configured on this cloud.", 503);
  }
  const limit = await c.env.TURN_LIMITER.limit({ key: comp.owner_user_id! });
  if (!limit.success) throw new ApiError("rateLimited", "Too many screen connections. Try again in a minute.");
  const ttl = 3600;
  // Cloudflare limits customIdentifier to 64 characters. Keep the computer ID
  // and a stable 128-bit device fingerprint (45 characters total).
  const deviceFingerprint = b64url((await sha256(fromB64url(dk, 32)!)).subarray(0, 16));
  try {
    const res = await fetch(
      `https://rtc.live.cloudflare.com/v1/turn/keys/${encodeURIComponent(c.env.TURN_KEY_ID)}/credentials/generate-ice-servers`,
      {
        method: "POST",
        headers: { Authorization: `Bearer ${c.env.TURN_KEY_API_TOKEN}`, "Content-Type": "application/json" },
        body: JSON.stringify({ ttl, customIdentifier: `${comp.id}:${deviceFingerprint}` }),
        signal: AbortSignal.timeout(8000),
      },
    );
    if (!res.ok) throw new Error("TURN provider refused");
    const data = await res.json() as { iceServers?: unknown };
    if (!Array.isArray(data.iceServers) || !data.iceServers.length || data.iceServers.length > 8) throw new Error("Invalid TURN response");
    const iceServers = data.iceServers.map((entry: { urls?: unknown; username?: unknown; credential?: unknown }) => {
      if (!entry || !Array.isArray(entry.urls) || !entry.urls.length || !entry.urls.every(
        (url: unknown) => typeof url === "string" && /^(stun:stun|turns?:turn)\.cloudflare\.com:\d+(\?transport=(udp|tcp))?$/.test(url),
      )) throw new Error("Invalid TURN servers");
      const turn = entry.urls.some((url: string) => url.startsWith("turn"));
      if (turn && (typeof entry.username !== "string" || !entry.username || typeof entry.credential !== "string" || !entry.credential)) {
        throw new Error("Invalid TURN credentials");
      }
      return { urls: entry.urls, ...(turn ? { username: entry.username, credential: entry.credential } : {}) };
    });
    if (!iceServers.some((s) => s.urls.some((u: string) => u.startsWith("turn")))) throw new Error("Missing TURN server");
    return Response.json({ iceServers, expiresAt: c.now + ttl * 1000 }, { headers: { "Cache-Control": "no-store" } });
  } catch {
    // Provider bodies and credential-bearing errors must never reach logs or clients.
    throw new ApiError("screenRelayUnavailable", "Remote screen relay is temporarily unavailable. Try again shortly.", 503);
  }
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
  // The account's phones that signed out, and how long ago (relative, so the host's clock needn't match this one): the
  // host drops their push tickets from before then, so a ticket sent after signing back in survives a pull that lands
  // before the phone's `POST /v1/devices` clears the mark.
  const signedOut = comp.owner_user_id
    ? await c.env.DB.prepare(
        "SELECT sign_pub, signed_out_at FROM devices WHERE owner_user_id = ? AND signed_out_at IS NOT NULL AND revoked_at IS NULL",
      )
        .bind(comp.owner_user_id)
        .all<{ sign_pub: string; signed_out_at: number }>()
    : { results: [] };
  return {
    owner: owner ? { userId: owner.user_id, email: owner.email } : null,
    signedOutDevices: signedOut.results.map((d) => ({ deviceKey: d.sign_pub, signedOutAgo: c.now - d.signed_out_at })),
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
    // A cloud computer's name, and its update that can't wait (`remote::managed` on the host installs it at once).
    ...(await cloudFor(c, comp)),
  };
}

async function cloudFor(c: Ctx, comp: Awaited<ReturnType<typeof host>>) {
  const machine = await machineFor(c.env, comp.id);
  if (!machine) return {};
  const update = await urgentUpdate(c.env, comp, c.now);
  return { name: machine.name, ...(update ? { update } : {}) };
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
