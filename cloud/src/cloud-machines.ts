// Codync Cloud machines: their D1 rows and the provider calls behind them, shared by the routes, the cron
// and each computer's relay, which wakes a sleeping one (docs/plans/codync-cloud.md). Every call goes out with the
// owner's own key (`cloud-credentials.ts`).

import { boat, type Machine, type Size } from "../machines/src";
import { forgetCredential, GROUP, type Provider, providerSend } from "./cloud-credentials";
import { machineName } from "./cloud-setup";
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
  created_at: number;
  updated_at: number;
}

/** The size every cloud computer gets for now. */
export const SIZE: Size = "small";

/** Asleep this long, a machine gets a backup that never expires (the provider drops its own after 14 days). */
const BACKUP_AFTER_MS = 10 * 86_400_000;

/** The cloud machine behind a claimed computer, if Codync runs it. */
export const machineFor = (env: Env, computerId: string) =>
  env.DB.prepare("SELECT * FROM cloud_machines WHERE computer_id = ? AND state != 'deleted'").bind(computerId).first<CloudMachineRow>();

/** The user's cloud machine (one per account), if they have one. */
export const machineOf = (env: Env, userId: string) =>
  env.DB.prepare("SELECT * FROM cloud_machines WHERE user_id = ? AND state != 'deleted'").bind(userId).first<CloudMachineRow>();

export const setState = (env: Env, id: string, state: Machine["state"], now: number) =>
  env.DB.prepare("UPDATE cloud_machines SET state = ?, updated_at = ? WHERE id = ?").bind(state, now, id).run();

export const autoStop = (env: Env) => (env.CLOUD_AUTO_STOP_SECONDS ? Number(env.CLOUD_AUTO_STOP_SECONDS) : undefined);

/** `started`: it is booting now. `running`: it never stopped. `retry`: the provider can't start it yet (still
 * stopping, start limit reached, unreachable). `none`: not a cloud computer, or one that can't be started (its owner's
 * key was refused, or it failed): only its owner can change that, so nothing retries. */
export type WakeResult = "none" | "started" | "running" | "retry";

/** Starts the sleeping cloud machine behind `computerId`. */
export async function wake(env: Env, computerId: string, now: number): Promise<WakeResult> {
  const m = await machineFor(env, computerId);
  if (!m) return "none";
  const live = boat.machine(await providerSend(env, m.user_id, m.provider, boat.get(m.provider_id), now));
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
  const resumed = boat.machine(await providerSend(env, m.user_id, m.provider, boat.resume(m.provider_id), now));
  if (resumed.ok) {
    await setState(env, m.id, resumed.value.state, now);
    return "started";
  }
  if (resumed.code === boat.SNAPSHOT_EXPIRED && m.backup) return fromBackup(env, m, now);
  console.log(JSON.stringify({ msg: "cloud computer resume refused", computerId, code: resumed.code }));
  return resumed.needsOwner ? "none" : "retry";
}

/** The machine's own snapshot expired: a new one from its backup takes its place (same disk, new provider id). */
async function fromBackup(env: Env, m: CloudMachineRow, now: number): Promise<WakeResult> {
  const spec = { name: machineName(env, m.user_id), size: SIZE, group: GROUP, autoStopSeconds: autoStop(env) };
  const made = boat.machine(await providerSend(env, m.user_id, m.provider, boat.fromBackup(spec, m.backup!, `${m.id}-${m.backup_at}`), now));
  if (!made.ok) {
    console.error(JSON.stringify({ msg: "cloud computer restore from backup failed", machine: m.id, code: made.code }));
    return made.retryable ? "retry" : "none";
  }
  await env.DB.prepare("UPDATE cloud_machines SET provider_id = ?, state = ?, updated_at = ? WHERE id = ?")
    .bind(made.value.id, made.value.state, now, m.id)
    .run();
  return "started";
}

/** Cron: a machine asleep for 10 days gets a backup (again, if it woke since the last one). */
export async function saveBackups(env: Env, now: number): Promise<void> {
  const { results } = await env.DB.prepare(
    `SELECT * FROM cloud_machines WHERE state IN ('stopping', 'stopped') AND updated_at < ?
       AND (backup_at IS NULL OR backup_at < updated_at)`,
  )
    .bind(now - BACKUP_AFTER_MS)
    .all<CloudMachineRow>();
  for (const m of results) {
    const name = m.backup ?? `codync-${crypto.randomUUID()}`;
    const saved = boat.done(await providerSend(env, m.user_id, m.provider, boat.saveBackup(m.provider_id, name), now));
    if (!saved.ok) {
      console.error(JSON.stringify({ msg: "cloud computer backup failed", machine: m.id, code: saved.code }));
      continue;
    }
    await env.DB.prepare("UPDATE cloud_machines SET backup = ?, backup_at = ? WHERE id = ?").bind(name, now, m.id).run();
  }
}

/** Deletes a machine's backup, if it has one; the backup outlives the machine otherwise. */
export async function dropBackup(env: Env, m: CloudMachineRow): Promise<boolean> {
  if (!m.backup) return true;
  const gone = boat.done(await providerSend(env, m.user_id, m.provider, boat.deleteBackup(m.backup)));
  if (!gone.ok && gone.code !== "http404") return false;
  await env.DB.prepare("UPDATE cloud_machines SET backup = NULL, backup_at = NULL WHERE id = ?").bind(m.id).run();
  return true;
}

/**
 * The owner's Codync account is gone. Their cloud computer is on their own provider account, which keeps it until they
 * delete it there: Codync stops it so it stops billing them, then forgets it and their key. Throws while the provider
 * can't take the stop yet (busy, unreachable), so the account deletion is retried before the key is gone.
 */
export async function forgetCloud(env: Env, userId: string, now: number): Promise<void> {
  const m = await machineOf(env, userId);
  if (m && m.state !== "stopped") {
    const stopped = boat.machine(await providerSend(env, userId, m.provider, boat.stop(m.provider_id), now));
    if (!stopped.ok && stopped.retryable) throw new Error(`cloud computer ${m.id} couldn't be stopped yet: ${stopped.code}`);
    if (!stopped.ok) console.error(JSON.stringify({ msg: "cloud computer left running", machine: m.id, code: stopped.code }));
  }
  await env.DB.batch([
    env.DB.prepare("UPDATE cloud_machines SET state = 'deleted', updated_at = ? WHERE user_id = ? AND state != 'deleted'").bind(now, userId),
    forgetCredential(env, userId, null),
  ]);
}
