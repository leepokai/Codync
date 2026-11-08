// Codync Cloud machines: their D1 rows and the provider calls behind them, shared by the routes, the cron
// and each computer's relay, which wakes a sleeping one (docs/plans/codync-cloud.md). Every call goes out with the
// owner's own key (`cloud-credentials.ts`).

import { drivers, type Machine, type Outcome, type Size } from "../machines/src";
import { randomId } from "./auth";
import { forgetCredential, GROUP, type Provider, providerSend } from "./cloud-credentials";
import { machineName, machineSetup } from "./cloud-setup";
import type { Env } from "./index";

export interface CloudMachineRow {
  id: string;
  user_id: string;
  provider: Provider;
  provider_id: string;
  claim_id: string;
  computer_id: string | null;
  state: Machine["state"];
  backup: string | null;
  backup_at: number | null;
  /** When it was given its files and setup script (with the create, or once it ran); null until then. */
  setup_at: number | null;
  created_at: number;
  updated_at: number;
}

/** The size every cloud computer gets for now. */
export const SIZE: Size = "small";

const DAY_MS = 86_400_000;

/** A cloud machine's claim ticket: long enough for it to boot, install the host and register. */
export const CLOUD_CLAIM_TTL_MS = 30 * 60_000;

/** A one-time ticket a host claims itself to `userId` with, until `expiresAt` (a computer's desktop app, or a cloud
 * machine's setup). */
export async function newClaim(env: Env, userId: string, expiresAt: number): Promise<{ claimId: string; nonce: string }> {
  const claimId = randomId("clm_");
  const nonce = randomId();
  await env.DB.prepare("INSERT INTO claims(id, user_id, nonce, expires_at) VALUES (?, ?, ?, ?)").bind(claimId, userId, nonce, expiresAt).run();
  return { claimId, nonce };
}

/** The cloud machine behind a claimed computer, if Codync runs it. */
export const machineFor = (env: Env, computerId: string) =>
  env.DB.prepare("SELECT * FROM cloud_machines WHERE computer_id = ? AND state != 'deleted'").bind(computerId).first<CloudMachineRow>();

/** The user's cloud machine (one per account), if they have one. */
export const machineOf = (env: Env, userId: string) =>
  env.DB.prepare("SELECT * FROM cloud_machines WHERE user_id = ? AND state != 'deleted'").bind(userId).first<CloudMachineRow>();

export const setState = (env: Env, id: string, state: Machine["state"], now: number) =>
  env.DB.prepare("UPDATE cloud_machines SET state = ?, updated_at = ? WHERE id = ?").bind(state, now, id).run();

export const autoStop = (env: Env) => (env.CLOUD_AUTO_STOP_SECONDS ? Number(env.CLOUD_AUTO_STOP_SECONDS) : undefined);

/** The machine as its provider sees it now. */
export async function liveMachine(env: Env, m: Pick<CloudMachineRow, "user_id" | "provider" | "provider_id">, now: number): Promise<Outcome<Machine>> {
  const d = drivers[m.provider];
  return d.machine(await providerSend(env, m.user_id, m.provider, d.get(m.provider_id), now));
}

/** `liveMachine`, with its state recorded (on `m` too) when it changed. */
export async function refresh(env: Env, m: CloudMachineRow, now: number): Promise<Outcome<Machine>> {
  const live = await liveMachine(env, m, now);
  if (live.ok && live.value.state !== m.state) {
    m.state = live.value.state;
    await setState(env, m.id, m.state, now);
  }
  return live;
}

/** `started`: it is booting now. `running`: it never stopped. `retry`: the provider can't start it yet (still
 * stopping, start limit reached, unreachable). `none`: not a cloud computer, or one that can't be started (its owner's
 * key was refused, or it failed): only its owner can change that, so nothing retries. */
export type WakeResult = "none" | "started" | "running" | "retry";

/** Starts the sleeping cloud machine behind `computerId`. */
export async function wake(env: Env, computerId: string, now: number): Promise<WakeResult> {
  const m = await machineFor(env, computerId);
  if (!m) return "none";
  const d = drivers[m.provider];
  const live = await liveMachine(env, m, now);
  if (!live.ok) return live.needsOwner ? "none" : "retry";
  const state = live.value.state;
  if (state === "running" || state === "starting") {
    if (state !== m.state) await setState(env, m.id, state, now);
    return state === "running" ? "running" : "started";
  }
  if (state === "stopping") return "retry";
  if (state !== "stopped") {
    console.error(JSON.stringify({ msg: "cloud computer can't wake", computerId, state }));
    return "none";
  }
  const resumed = d.machine(await providerSend(env, m.user_id, m.provider, d.wake(m.provider_id, crypto.randomUUID()), now));
  if (resumed.ok) {
    await setState(env, m.id, resumed.value.state, now);
    return "started";
  }
  if (resumed.code === "backupNeeded" && m.backup) return fromBackup(env, m, now);
  console.log(JSON.stringify({ msg: "cloud computer resume refused", computerId, code: resumed.detail }));
  return resumed.needsOwner ? "none" : "retry";
}

/** The machine's own snapshot expired: a new one from its backup takes its place (same disk, new provider id). */
async function fromBackup(env: Env, m: CloudMachineRow, now: number): Promise<WakeResult> {
  const backup = drivers[m.provider].backup;
  if (!backup) return "none";
  const spec = { name: machineName(env, m.user_id), size: SIZE, group: GROUP, autoStopSeconds: autoStop(env) };
  const made = drivers[m.provider].machine(await providerSend(env, m.user_id, m.provider, backup.restore(spec, m.backup!, `${m.id}-${m.backup_at}`), now));
  if (!made.ok) {
    console.error(JSON.stringify({ msg: "cloud computer restore from backup failed", machine: m.id, code: made.detail }));
    return made.retryable ? "retry" : "none";
  }
  await env.DB.prepare("UPDATE cloud_machines SET provider_id = ?, state = ?, updated_at = ? WHERE id = ?")
    .bind(made.value.id, made.value.state, now, m.id)
    .run();
  return "started";
}

/** Cron: a machine asleep for its provider's `backup.beforeDays` gets a backup (again, if it woke since the last one);
 * one whose provider keeps a stopped disk until it is deleted needs none. */
export async function saveBackups(env: Env, now: number): Promise<void> {
  for (const d of Object.values(drivers)) {
    if (!d.backup) continue;
    const { results } = await env.DB.prepare(
      `SELECT * FROM cloud_machines WHERE provider = ? AND state IN ('stopping', 'stopped') AND updated_at < ?
         AND (backup_at IS NULL OR backup_at < updated_at)`,
    )
      .bind(d.id, now - d.backup.beforeDays * DAY_MS)
      .all<CloudMachineRow>();
    for (const m of results) {
      const name = m.backup ?? `codync-${crypto.randomUUID()}`;
      const saved = d.done(await providerSend(env, m.user_id, m.provider, d.backup.save(m.provider_id, name), now));
      if (!saved.ok) {
        console.error(JSON.stringify({ msg: "cloud computer backup failed", machine: m.id, code: saved.detail }));
        continue;
      }
      await env.DB.prepare("UPDATE cloud_machines SET backup = ?, backup_at = ? WHERE id = ?").bind(name, now, m.id).run();
    }
  }
}

/** Deletes a machine's backup, if it has one; the backup outlives the machine otherwise. */
export async function dropBackup(env: Env, m: CloudMachineRow): Promise<boolean> {
  const backup = drivers[m.provider].backup;
  if (!m.backup || !backup) return true;
  const gone = drivers[m.provider].done(await providerSend(env, m.user_id, m.provider, backup.drop(m.backup)));
  if (!gone.ok && gone.code !== "notFound") return false;
  await env.DB.prepare("UPDATE cloud_machines SET backup = NULL, backup_at = NULL WHERE id = ?").bind(m.id).run();
  return true;
}

/**
 * Gives a machine whose provider takes nothing at create (its driver has `setup`) a claim ticket and its setup script,
 * once it runs (`live`): on the apps' reads while it prepares (every few seconds), or the cron's. The ticket is a new
 * one, so the host has its whole life to install and claim however long the machine took. One read takes it at a time
 * (`setup_at` is set first, and cleared again if the setup didn't go out), so two apps reading at once never install
 * the host twice. One its provider paused before it got them is started again, and set up on a later read.
 */
export async function setUp(env: Env, m: CloudMachineRow, live: Machine["state"], now: number): Promise<void> {
  const d = drivers[m.provider];
  if (m.setup_at !== null || !d.setup) return;
  if (live === "stopped") {
    const woke = d.machine(await providerSend(env, m.user_id, m.provider, d.wake(m.provider_id, crypto.randomUUID()), now));
    if (!woke.ok) console.error(JSON.stringify({ msg: "cloud computer can't wake for its setup", machine: m.id, code: woke.detail }));
    return;
  }
  if (live !== "running") return;
  const took = await env.DB.prepare("UPDATE cloud_machines SET setup_at = ? WHERE id = ? AND setup_at IS NULL").bind(now, m.id).run();
  if (!took.meta.changes) return;
  const release = () => env.DB.prepare("UPDATE cloud_machines SET setup_at = NULL WHERE id = ? AND setup_at = ?").bind(m.id, now).run();
  try {
    const { claimId, nonce } = await newClaim(env, m.user_id, now + CLOUD_CLAIM_TTL_MS);
    await env.DB.prepare("UPDATE cloud_machines SET claim_id = ? WHERE id = ?").bind(claimId, m.id).run();
    const sent = d.done(await providerSend(env, m.user_id, m.provider, d.setup(m.provider_id, machineSetup(env, claimId, nonce, m.user_id)), now));
    if (sent.ok) return;
    console.error(JSON.stringify({ msg: "cloud computer setup failed", machine: m.id, code: sent.detail }));
  } catch (e) {
    await release();
    throw e;
  }
  await release();
}

/** Cron: machines still waiting for their setup, each on its own (one that throws doesn't hold up the rest). */
export async function setUpPending(env: Env, now: number): Promise<void> {
  const { results } = await env.DB.prepare("SELECT * FROM cloud_machines WHERE setup_at IS NULL AND state NOT IN ('deleted', 'failed')").all<CloudMachineRow>();
  for (const m of results) {
    try {
      const live = await refresh(env, m, now);
      if (live.ok) await setUp(env, m, live.value.state, now);
    } catch (e) {
      console.error(JSON.stringify({ msg: "cloud computer setup failed", machine: m.id, error: String(e) }));
    }
  }
}

/**
 * The owner's Codync account is gone. Their cloud computer is on their own provider account, which keeps it until they
 * delete it there: Codync stops it so it stops billing them, then forgets it and their key. Throws while the provider
 * can't take the stop yet (busy, unreachable), so the account deletion is retried before the key is gone.
 */
export async function forgetCloud(env: Env, userId: string, now: number): Promise<void> {
  const m = await machineOf(env, userId);
  if (m && m.state !== "stopped") {
    const d = drivers[m.provider];
    // Read first: a provider may have put it to sleep on its own, and a second sleep is refused there.
    const live = await liveMachine(env, m, now);
    const awake = live.ok && (live.value.state === "running" || live.value.state === "starting");
    const stopped = awake ? d.machine(await providerSend(env, userId, m.provider, d.sleep(m.provider_id, crypto.randomUUID()), now)) : live;
    if (!stopped.ok && stopped.retryable) throw new Error(`cloud computer ${m.id} couldn't be stopped yet: ${stopped.detail}`);
    if (!stopped.ok && stopped.code !== "notFound") console.error(JSON.stringify({ msg: "cloud computer left running", machine: m.id, code: stopped.detail }));
  }
  await env.DB.batch([
    env.DB.prepare("UPDATE cloud_machines SET state = 'deleted', updated_at = ? WHERE user_id = ? AND state != 'deleted'").bind(now, userId),
    forgetCredential(env, userId, null),
  ]);
}
