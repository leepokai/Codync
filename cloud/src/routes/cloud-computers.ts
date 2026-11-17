// Codync Cloud: cloud computers Codync runs for a signed-in user on their own provider accounts (a few per account,
// each on the provider the user picks), with the keys they gave it (docs/plans/codync-cloud.md; the keys' routes are in
// `cloud-credentials.ts`). The Worker creates the
// machine with a one-time claim ticket on its disk (in the create call, or once it runs: `setUp`); the host claims
// itself on first boot (`hostClaim`) and then shows up in the account's computers like any other. Its host-facing
// routes (sleep, updates) are in `cloud-host.ts`.

import { drivers, type Outcome } from "../../machines/src";
import { randomId } from "../auth";
import { credentialsOf, GROUP, isProvider, problemText, PROVIDERS, providersView, providerSend } from "../cloud-credentials";
import {
  autoStop,
  CLOUD_CLAIM_TTL_MS,
  CLOUD_COMPUTER_LIMIT,
  type CloudMachineRow,
  dropBackup,
  liveMachine,
  machineDeleted,
  machineOf,
  machinesOf,
  nameComputer,
  newClaim,
  newVault,
  settle,
  SIZE,
} from "../cloud-machines";
import { machineName, machineSetup } from "../cloud-setup";
import { canSeal } from "../secrets";
import { ApiError, audit, body, type Ctx, notify, optStr, unclaim, user } from "./common";

/** What the apps show: also in `/v1/computers` as each cloud computer's `cloud`. */
export const cloudView = (r: Pick<CloudMachineRow, "provider" | "state">) => ({
  provider: r.provider,
  providerName: PROVIDERS[r.provider].name,
  region: PROVIDERS[r.provider].region,
  state: r.state,
});

const view = (r: CloudMachineRow) => ({
  cloudComputerId: r.id,
  name: r.name,
  ...cloudView(r),
  computerId: r.computer_id,
  backupAt: r.backup_at,
  // Whether *Delete snapshots* has anything to erase: the provider's history of earlier states, or a backup.
  snapshots: !!(drivers[r.provider].deleteHistory || drivers[r.provider].backup),
  createdAt: r.created_at,
});

/** The signed-in user, on a Worker that can seal their keys (`CLOUD_SECRET_KEYS`): every account may then have one. */
export async function cloudUser(c: Ctx) {
  const u = await user(c);
  if (!canSeal(c.env)) throw new ApiError("forbidden", "Codync Cloud isn't available here.");
  return u;
}

/** The provider busy or failing: another try may work. */
export const unreachable = () => new ApiError("providerUnavailable", "Couldn't reach the cloud provider. Try again in a minute.");

/** The outcome's value, or the error the apps show for it; `refused`: what to say when the provider just said no. */
export function value<T>(o: Outcome<T>, refused = "The cloud provider couldn't do this right now. Try again in a minute."): T {
  if (o.ok) return o.value;
  console.log(JSON.stringify({ msg: "cloud computer provider refused", code: o.detail, message: o.message }));
  if (o.retryable) throw unreachable();
  // The apps show the key's problem too; this says it where the user just tapped.
  if (o.needsOwner) throw new ApiError("keyRefused", problemText(o.code));
  throw new ApiError("internal", refused);
}

async function mine(c: Ctx, id: string) {
  const u = await user(c);
  const row = await machineOf(c.env, u.userId, id);
  if (!row) throw new ApiError("notFound");
  return { u, row };
}

/** A name the user typed: trimmed, at most 64 characters. */
function nameOf(c: Ctx): string | null {
  const name = optStr(body(c), "name", 64)?.trim();
  return name || null;
}

/**
 * `POST /v1/cloud-computers {provider?, name?}`: creates another cloud computer for this user, up to
 * `CLOUD_COMPUTER_LIMIT`, on one of the providers they gave a key (`provider`; may be left out when there is only one).
 * `name` (default "Cloud computer") is what every app shows for it.
 */
export async function createCloudComputer(c: Ctx) {
  const u = await cloudUser(c);
  const name = nameOf(c) ?? "Cloud computer";
  // ponytail: two creates at the same instant can both pass the count; the limit is a guard rail, not billing.
  if ((await machinesOf(c.env, u.userId)).length >= CLOUD_COMPUTER_LIMIT) {
    throw new ApiError("conflict", `You can have up to ${CLOUD_COMPUTER_LIMIT} cloud computers. Delete one to make another.`);
  }
  const asked = body(c).provider;
  if (asked !== undefined && !isProvider(asked)) throw new ApiError("badRequest", "Invalid provider");
  // A key the provider turned down earlier is tried again: the owner may have fixed it there (their plan, say).
  const keys = await credentialsOf(c.env, u.userId);
  const key = asked ? keys.find((k) => k.provider === asked) : keys.length === 1 ? keys[0] : undefined;
  if (!key) throw new ApiError("conflict", keys.length ? "Choose the provider to create it on." : "Connect a cloud provider first.");

  const { claimId, nonce } = await newClaim(c.env, u.userId, c.now + CLOUD_CLAIM_TTL_MS);
  const d = drivers[key.provider];
  const id = randomId("cm_");
  const spec = {
    name: machineName(c.env, u.userId, id),
    size: SIZE,
    group: GROUP,
    ...machineSetup(c.env, claimId, nonce, u.userId),
    autoStopSeconds: autoStop(c.env),
  };
  const machine = value(d.machine(await providerSend(c.env, u.userId, key.provider, d.create(spec, claimId), c.now)));
  const vault = await newVault(c.env, id);
  const row: CloudMachineRow = {
    id,
    user_id: u.userId,
    name,
    provider: key.provider,
    provider_id: machine.id,
    claim_id: claimId,
    computer_id: null,
    state: machine.state,
    backup: null,
    backup_at: null,
    // Set up by the create, unless its provider takes nothing then (`setUp`, once it runs).
    setup_at: d.setup ? null : c.now,
    vault_sealed: vault.sealed,
    vault_key_id: vault.keyId,
    keeper_key: null,
    // Its host says once it asks to sleep.
    analytics: null,
    created_at: c.now,
    updated_at: c.now,
  };
  await c.env.DB.batch([
    c.env.DB.prepare(
      `INSERT INTO cloud_machines(id, user_id, name, provider, provider_id, claim_id, state, setup_at, vault_sealed, vault_key_id, created_at, updated_at)
       VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)`,
    ).bind(row.id, row.user_id, row.name, row.provider, row.provider_id, row.claim_id, row.state, row.setup_at, row.vault_sealed, row.vault_key_id, c.now),
    audit(c.env, u.userId, "cloudComputer.create", row.id, "ok", c.now),
  ]);
  return { cloudComputer: view(row) };
}

/** `GET /v1/cloud-computers`: this user's cloud computers with their live state, oldest first; `available` says whether
 * this account may create them (the apps show Codync Cloud only then, or when it has one), `limit` how many it may
 * have; `providers`, every provider they can run on, each with how to make its key and the user's key for it as the
 * apps may see it (hint and problem). */
export async function getCloudComputers(c: Ctx) {
  const u = await user(c);
  const rows = await machinesOf(c.env, u.userId);
  // Each at once, on its own: a provider out of reach leaves its machines as last seen, and the others still show.
  await Promise.all(rows.map((row) => settle(c.env, row, c.now)));
  const open = rows.length > 0 || canSeal(c.env);
  return {
    cloudComputers: rows.map(view),
    available: open,
    limit: CLOUD_COMPUTER_LIMIT,
    // Read after the live check, which records a key the provider just turned down.
    providers: open ? providersView(await credentialsOf(c.env, u.userId)) : [],
  };
}

/** `PATCH /v1/cloud-computers/:id {name}`: renames it, here and as its computer; its host takes the name at its next
 * look at the cloud, which this asks for. */
export async function renameCloudComputer(c: Ctx, [id]: string[]) {
  const { u, row } = await mine(c, id!);
  const name = nameOf(c);
  if (!name) throw new ApiError("badRequest", "Invalid name");
  row.name = name;
  await c.env.DB.batch([
    // `updated_at` stays: for a sleeping machine it is when it went to sleep, which its backup is timed from.
    c.env.DB.prepare("UPDATE cloud_machines SET name = ? WHERE id = ?").bind(name, row.id),
    ...(row.computer_id ? [nameComputer(c.env, row.computer_id, name, c.now)] : []),
    audit(c.env, u.userId, "cloudComputer.rename", row.id, "ok", c.now),
  ]);
  if (row.computer_id) await notify(c.env, row.computer_id);
  return { cloudComputer: view(row) };
}

const BACKUP_KEPT = "The backup couldn't be deleted. Try again in a minute.";

/** `DELETE /v1/cloud-computers/:id`: erases the machine, its snapshots and its backup, and takes the computer out
 * of the account. The backup goes first: one that can't be deleted yet keeps the computer listed, so deleting it can
 * be tried again (a backup left behind would never expire). */
export async function deleteCloudComputer(c: Ctx, [id]: string[]) {
  const { u, row } = await mine(c, id!);
  const d = drivers[row.provider];
  value(await dropBackup(c.env, row), BACKUP_KEPT);
  value(d.deleted(await providerSend(c.env, u.userId, row.provider, d.destroy(row.provider_id, crypto.randomUUID()), c.now)));
  if (row.computer_id) await unclaim(c.env, row.computer_id, u.userId, c.now);
  await c.env.DB.batch([
    machineDeleted(c.env, row.id, c.now),
    audit(c.env, u.userId, "cloudComputer.delete", row.id, "ok", c.now),
  ]);
  return {};
}

/**
 * `DELETE /v1/cloud-computers/:id/snapshots`: erases the computer's saved history (the provider's snapshots of
 * earlier states, and its backup) and keeps the computer. Only while it is awake: asleep, its latest snapshot
 * is the computer itself (deleting the computer erases everything). A provider that keeps no history has only the
 * backup to erase, if any.
 */
export async function deleteCloudSnapshots(c: Ctx, [id]: string[]) {
  const { u, row } = await mine(c, id!);
  const d = drivers[row.provider];
  if (d.deleteHistory) {
    const live = value(await liveMachine(c.env, row, c.now));
    if (live.state !== "running") {
      throw new ApiError("conflict", "Your cloud computer is asleep, and its snapshot is all it has. Open it first, or delete the computer.");
    }
    value(d.done(await providerSend(c.env, u.userId, row.provider, d.deleteHistory(row.provider_id), c.now)));
  }
  value(await dropBackup(c.env, row), BACKUP_KEPT);
  await audit(c.env, u.userId, "cloudComputer.deleteSnapshots", row.id, "ok", c.now).run();
  return {};
}
