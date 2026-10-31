// Codync Cloud: a cloud computer Codync runs for a signed-in user (docs/plans/codync-cloud.md). The Worker
// creates the machine with a one-time claim ticket on its disk; the host claims itself on first boot
// (`hostClaim`) and then shows up in the account's computers like any other. Its host-facing routes (sleep,
// updates) are in `cloud-host.ts`.

import { boat, type Outcome } from "../../machines/src";
import { randomId } from "../auth";
import { autoStop, type CloudMachineRow, dropBackup, PROVIDERS, providerSend, setState, SIZE } from "../cloud-machines";
import { machineName, setupScript } from "../cloud-setup";
import { ApiError, audit, unclaim, user, type Ctx } from "./common";

/** Long enough for the machine to boot, install the host and register. */
const CLAIM_TTL_MS = 30 * 60_000;

/** What the apps show: also in `/v1/computers` as each cloud computer's `cloud`. */
export const cloudView = (r: Pick<CloudMachineRow, "provider" | "state">) => ({
  provider: r.provider,
  providerName: PROVIDERS[r.provider].name,
  region: PROVIDERS[r.provider].region,
  state: r.state,
});

const view = (r: CloudMachineRow) => ({
  cloudComputerId: r.id,
  ...cloudView(r),
  computerId: r.computer_id,
  backupAt: r.backup_at,
  createdAt: r.created_at,
});

/** Payments are set aside: while they are, an allowlist of Clerk user ids decides who may have one (`*`: everyone,
 * for a local test cloud only). */
const available = (c: Ctx, userId: string) => {
  const allowed = (c.env.CLOUD_COMPUTER_USERS ?? "").split(",").map((s) => s.trim());
  return !!c.env.BOAT_API_KEY && (allowed.includes(userId) || allowed.includes("*"));
};

async function allowedUser(c: Ctx) {
  const u = await user(c);
  if (!available(c, u.userId)) throw new ApiError("forbidden", "Codync Cloud isn't available for this account yet.");
  return u;
}

export function value<T>(o: Outcome<T>): T {
  if (o.ok) return o.value;
  console.log(JSON.stringify({ msg: "cloud computer provider refused", code: o.code, message: o.message }));
  throw new ApiError(o.retryable ? "rateLimited" : "internal", "The cloud provider couldn't do this right now. Try again in a minute.");
}

const current = (c: Ctx, userId: string) =>
  c.env.DB.prepare("SELECT * FROM cloud_machines WHERE user_id = ? AND state != 'deleted'").bind(userId).first<CloudMachineRow>();

async function mine(c: Ctx) {
  const u = await user(c);
  const row = await current(c, u.userId);
  if (!row) throw new ApiError("notFound");
  return { u, row };
}

/** `POST /v1/cloud-computers`: creates this user's cloud computer, or returns the one they have. */
export async function createCloudComputer(c: Ctx) {
  const u = await allowedUser(c);
  const existing = await current(c, u.userId);
  if (existing) return { cloudComputer: view(existing) };

  const claimId = randomId("clm_");
  const nonce = randomId();
  await c.env.DB.prepare("INSERT INTO claims(id, user_id, nonce, expires_at) VALUES (?, ?, ?, ?)")
    .bind(claimId, u.userId, nonce, c.now + CLAIM_TTL_MS)
    .run();
  const ticket = JSON.stringify({ claimId, nonce, userId: u.userId, approval: "auto" });
  const spec = {
    name: machineName(c.env, u.userId),
    size: SIZE,
    files: [{ path: ".codync/managed.json", content: ticket }],
    setupScript: setupScript(c.env),
    autoStopSeconds: autoStop(c.env),
  };
  const machine = value(boat.machine(await providerSend(c.env, boat.create(spec, claimId))));
  const row: CloudMachineRow = {
    id: randomId("cm_"),
    user_id: u.userId,
    provider: "boat",
    provider_id: machine.id,
    claim_id: claimId,
    computer_id: null,
    state: machine.state,
    backup: null,
    backup_at: null,
    created_at: c.now,
    updated_at: c.now,
  };
  await c.env.DB.batch([
    c.env.DB.prepare(
      `INSERT INTO cloud_machines(id, user_id, provider, provider_id, claim_id, state, created_at, updated_at)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)`,
    ).bind(row.id, row.user_id, row.provider, row.provider_id, row.claim_id, row.state, c.now),
    audit(c.env, u.userId, "cloudComputer.create", row.id, "ok", c.now),
  ]);
  return { cloudComputer: view(row) };
}

/** `GET /v1/cloud-computers`: this user's cloud computer with its live state, or `null`; `available` says whether
 * this account may create one (the apps show Codync Cloud only then, or when it has one). */
export async function getCloudComputer(c: Ctx) {
  const u = await user(c);
  const row = await current(c, u.userId);
  if (!row) return { cloudComputer: null, available: available(c, u.userId) };
  if (c.env.BOAT_API_KEY) {
    const live = boat.machine(await providerSend(c.env, boat.get(row.provider_id)));
    if (live.ok && live.value.state !== row.state) {
      row.state = live.value.state;
      await setState(c.env, row.id, row.state, c.now);
    }
  }
  return { cloudComputer: view(row), available: true };
}

/** `DELETE /v1/cloud-computers`: erases the machine, its snapshots and its backup, and takes the computer out
 * of the account. */
export async function deleteCloudComputer(c: Ctx) {
  const { u, row } = await mine(c);
  value(boat.deleted(await providerSend(c.env, boat.destroy(row.provider_id))));
  if (!(await dropBackup(c.env, row))) console.error(JSON.stringify({ msg: "cloud computer backup left behind", machine: row.id, backup: row.backup }));
  if (row.computer_id) await unclaim(c.env, row.computer_id, u.userId, c.now);
  await c.env.DB.batch([
    c.env.DB.prepare("UPDATE cloud_machines SET state = 'deleted', updated_at = ? WHERE id = ?").bind(c.now, row.id),
    audit(c.env, u.userId, "cloudComputer.delete", row.id, "ok", c.now),
  ]);
  return {};
}

/**
 * `DELETE /v1/cloud-computers/snapshots`: erases the computer's saved history (the provider's snapshots of
 * earlier states, and its backup) and keeps the computer. Only while it is awake: asleep, its latest snapshot
 * is the computer itself (deleting the computer erases everything).
 */
export async function deleteCloudSnapshots(c: Ctx) {
  const { u, row } = await mine(c);
  const live = value(boat.machine(await providerSend(c.env, boat.get(row.provider_id))));
  if (live.state !== "running") {
    throw new ApiError("conflict", "Your cloud computer is asleep, and its snapshot is all it has. Open it first, or delete the computer.");
  }
  value(boat.done(await providerSend(c.env, boat.deleteSnapshots(row.provider_id))));
  if (!(await dropBackup(c.env, row))) throw new ApiError("internal", "The backup couldn't be deleted. Try again in a minute.");
  await audit(c.env, u.userId, "cloudComputer.deleteSnapshots", row.id, "ok", c.now).run();
  return {};
}
