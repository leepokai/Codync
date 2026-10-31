// Codync Cloud machines: their D1 rows and the provider calls behind them, shared by the routes, the cron
// and each computer's relay, which wakes a sleeping one (docs/plans/codync-cloud.md).

import { boat, type HttpRequest, type HttpResponse, type Machine, type Size } from "../machines/src";
import { machineName } from "./cloud-setup";
import type { Env } from "./index";

export interface CloudMachineRow {
  id: string;
  user_id: string;
  provider: "boat";
  provider_id: string;
  claim_id: string;
  computer_id: string | null;
  state: Machine["state"];
  backup: string | null;
  backup_at: number | null;
  created_at: number;
  updated_at: number;
}

/** Shown in the apps: users see which provider runs their computer and where. */
export const PROVIDERS = { boat: { name: "Boat", region: "EU" } } as const;

/** The size every cloud computer gets for now. */
export const SIZE: Size = "small";

/** Asleep this long, a machine gets a backup that never expires (the provider drops its own after 14 days). */
const BACKUP_AFTER_MS = 10 * 86_400_000;

export async function providerSend(env: Env, req: HttpRequest): Promise<HttpResponse> {
  const base = env.BOAT_API_URL ?? boat.BOAT_API_URL;
  const res = await fetch(`${base}${req.path}`, {
    method: req.method,
    headers: { ...req.headers, authorization: `Bearer ${env.BOAT_API_KEY}`, "content-type": "application/json" },
    body: req.body === undefined ? undefined : JSON.stringify(req.body),
  });
  return { status: res.status, body: await res.json().catch(() => null) };
}

/** The cloud machine behind a claimed computer, if Codync runs it. */
export const machineFor = (env: Env, computerId: string) =>
  env.DB.prepare("SELECT * FROM cloud_machines WHERE computer_id = ? AND state != 'deleted'").bind(computerId).first<CloudMachineRow>();

export const setState = (env: Env, id: string, state: Machine["state"], now: number) =>
  env.DB.prepare("UPDATE cloud_machines SET state = ?, updated_at = ? WHERE id = ?").bind(state, now, id).run();

export const autoStop = (env: Env) => (env.CLOUD_AUTO_STOP_SECONDS ? Number(env.CLOUD_AUTO_STOP_SECONDS) : undefined);

/** `started`: it is booting now. `running`: it never stopped. `retry`: the provider can't start it yet (still
 * stopping, start limit reached, unreachable). `none`: not a cloud computer, or one that can't be started. */
export type WakeResult = "none" | "started" | "running" | "retry";

/** Starts the sleeping cloud machine behind `computerId`. */
export async function wake(env: Env, computerId: string, now: number): Promise<WakeResult> {
  if (!env.BOAT_API_KEY) return "none";
  const m = await machineFor(env, computerId);
  if (!m) return "none";
  const live = boat.machine(await providerSend(env, boat.get(m.provider_id)));
  if (!live.ok) return "retry";
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
  const resumed = boat.machine(await providerSend(env, boat.resume(m.provider_id)));
  if (resumed.ok) {
    await setState(env, m.id, resumed.value.state, now);
    return "started";
  }
  if (resumed.code === boat.SNAPSHOT_EXPIRED && m.backup) return fromBackup(env, m, now);
  console.log(JSON.stringify({ msg: "cloud computer resume refused", computerId, code: resumed.code }));
  return "retry";
}

/** The machine's own snapshot expired: a new one from its backup takes its place (same disk, new provider id). */
async function fromBackup(env: Env, m: CloudMachineRow, now: number): Promise<WakeResult> {
  const req = boat.fromBackup({ name: machineName(env, m.user_id), size: SIZE, autoStopSeconds: autoStop(env) }, m.backup!, `${m.id}-${m.backup_at}`);
  const made = boat.machine(await providerSend(env, req));
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
  if (!env.BOAT_API_KEY) return;
  const { results } = await env.DB.prepare(
    `SELECT * FROM cloud_machines WHERE state IN ('stopping', 'stopped') AND updated_at < ?
       AND (backup_at IS NULL OR backup_at < updated_at)`,
  )
    .bind(now - BACKUP_AFTER_MS)
    .all<CloudMachineRow>();
  for (const m of results) {
    const name = m.backup ?? `codync-${crypto.randomUUID()}`;
    const saved = boat.done(await providerSend(env, boat.saveBackup(m.provider_id, name)));
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
  const gone = boat.done(await providerSend(env, boat.deleteBackup(m.backup)));
  if (!gone.ok && gone.code !== "http404") return false;
  await env.DB.prepare("UPDATE cloud_machines SET backup = NULL, backup_at = NULL WHERE id = ?").bind(m.id).run();
  return true;
}
