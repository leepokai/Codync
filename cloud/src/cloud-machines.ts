// Codync Cloud machines: their D1 rows and the provider calls behind them, shared by the routes, the cron
// and each computer's relay, which wakes a sleeping one (docs/plans/codync-cloud.md). Every call goes out with the
// owner's own key (`cloud-credentials.ts`).

import { drivers, type Machine, type Outcome, type Size } from "../machines/src";
import { b64url, randomId } from "./auth";
import { forgetCredential, GROUP, type Provider, providerSend } from "./cloud-credentials";
import { machineName, machineSetup } from "./cloud-setup";
import type { Env } from "./index";
import { open, seal } from "./secrets";

export interface CloudMachineRow {
  id: string;
  user_id: string;
  /** The user's name for it, which its host takes as its computer's name. */
  name: string;
  provider: Provider;
  provider_id: string;
  claim_id: string;
  computer_id: string | null;
  state: Machine["state"];
  backup: string | null;
  backup_at: number | null;
  /** When it was given its files and setup script (with the create, or once it ran); null until then. */
  setup_at: number | null;
  /** The key to its keeper's vault, sealed (`newVault`); null once the machine is gone. */
  vault_sealed: string | null;
  vault_key_id: string | null;
  /** Its keeper's Ed25519 key (base64url), named by its claim; the only key the vault key goes to. */
  keeper_key: string | null;
  /** 1 while its host last said its owner shares usage analytics (`analytics.ts`); 0 or null: nothing is sent. */
  analytics: number | null;
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

/** What a machine's vault key is sealed for (with the machine's id as its owner). */
const VAULT_PURPOSE = "keeper-vault";

/** A new machine's key to its keeper's vault (32 random bytes, base64url), sealed: only its keeper ever gets it. */
export const newVault = (env: Env, id: string) => seal(env, id, VAULT_PURPOSE, b64url(crypto.getRandomValues(new Uint8Array(32))));

/** The machine's vault key; null once the machine is gone, or when it can't be opened. */
export const vaultKeyOf = (env: Env, m: Pick<CloudMachineRow, "id" | "vault_sealed" | "vault_key_id">) =>
  m.vault_sealed && m.vault_key_id ? open(env, m.id, VAULT_PURPOSE, { sealed: m.vault_sealed, keyId: m.vault_key_id }) : Promise.resolve(null);

/** The machine whose claim named keeper key `kid`. */
export const machineByKeeper = (env: Env, kid: string) =>
  env.DB.prepare("SELECT * FROM cloud_machines WHERE keeper_key = ? AND state != 'deleted'").bind(kid).first<CloudMachineRow>();

/** Marks machines deleted (those the `WHERE` names, with `?2`), erasing their vault keys: a copy of a disk kept
 * elsewhere can never fetch its key again. */
const markDeleted = (env: Env, where: string, value: string, now: number) =>
  env.DB.prepare(`UPDATE cloud_machines SET state = 'deleted', vault_sealed = NULL, vault_key_id = NULL, updated_at = ?1 WHERE ${where}`).bind(now, value);

/** Marks machine `id` deleted (`markDeleted`). */
export const machineDeleted = (env: Env, id: string, now: number) => markDeleted(env, "id = ?2", id, now);

/** The cloud machine behind a claimed computer, if Codync runs it. */
export const machineFor = (env: Env, computerId: string) =>
  env.DB.prepare("SELECT * FROM cloud_machines WHERE computer_id = ? AND state != 'deleted'").bind(computerId).first<CloudMachineRow>();

/** Records whether the host of `computerId` shares usage analytics, as it says when its owner turns the switch and with
 * each sleep request. */
export const setAnalytics = (env: Env, computerId: string, on: boolean) =>
  env.DB.prepare("UPDATE cloud_machines SET analytics = ? WHERE computer_id = ? AND state != 'deleted'").bind(on ? 1 : 0, computerId).run();

/** How many cloud computers one account may have at once. */
export const CLOUD_COMPUTER_LIMIT = 3;

/** The user's cloud machines, oldest first. */
export const machinesOf = async (env: Env, userId: string) =>
  (await env.DB.prepare("SELECT * FROM cloud_machines WHERE user_id = ? AND state != 'deleted' ORDER BY created_at").bind(userId).all<CloudMachineRow>())
    .results;

/** One of the user's cloud machines, by its id. */
export const machineOf = (env: Env, userId: string, id: string) =>
  env.DB.prepare("SELECT * FROM cloud_machines WHERE id = ? AND user_id = ? AND state != 'deleted'").bind(id, userId).first<CloudMachineRow>();

/** Gives a cloud machine's account computer the name its user gave the machine. */
export const nameComputer = (env: Env, computerId: string, name: string, now: number) =>
  env.DB.prepare("UPDATE computers SET name = ?, updated_at = ? WHERE id = ?").bind(name, now, computerId);

/** Records the machine's state, and `now` as when it entered it; except `stopping` → `stopped`, which keeps when it
 * went to sleep: its backup is timed from that (`saveBackups`), however late the stop is seen. */
export const setState = (env: Env, id: string, state: Machine["state"], now: number) =>
  env.DB.prepare(
    "UPDATE cloud_machines SET state = ?1, updated_at = CASE WHEN state = 'stopping' AND ?1 = 'stopped' THEN updated_at ELSE ?2 END WHERE id = ?3",
  )
    .bind(state, now, id)
    .run();

/** Puts machine `m` back as it was (its state, and when it entered it) when `hostSleep` recorded it `stopping` for a
 * sleep its relay refused; a state recorded since stays. */
export const undoStopping = (env: Env, m: CloudMachineRow) =>
  env.DB.prepare("UPDATE cloud_machines SET state = ?, updated_at = ? WHERE id = ? AND state = 'stopping'").bind(m.state, m.updated_at, m.id).run();

export const autoStop = (env: Env) => (env.CLOUD_AUTO_STOP_SECONDS ? Number(env.CLOUD_AUTO_STOP_SECONDS) : undefined);

/** The machine as its provider sees it now. */
export async function liveMachine(env: Env, m: Pick<CloudMachineRow, "user_id" | "provider" | "provider_id">, now: number): Promise<Outcome<Machine>> {
  const d = drivers[m.provider];
  return d.machine(await providerSend(env, m.user_id, m.provider, d.get(m.provider_id), now));
}

/** A stop takes effect within this long of the host asking to sleep (`hostSleep` records `stopping` then). */
export const STOP_SETTLE_MS = 2 * 60_000;

/** `liveMachine`, with its state recorded (on `m` too) when it changed; except a machine still awake right after its
 * host asked to sleep, which stays `stopping` until its stop had time to take effect: its host keeps waiting
 * (`GET /v1/host/sleep`), and a wake waits for the stop instead of taking the host back. */
export async function refresh(env: Env, m: CloudMachineRow, now: number): Promise<Outcome<Machine>> {
  const live = await liveMachine(env, m, now);
  if (!live.ok || live.value.state === m.state) return live;
  const awake = live.value.state === "running" || live.value.state === "starting";
  if (awake && m.state === "stopping" && now - m.updated_at < STOP_SETTLE_MS) return live;
  m.state = live.value.state;
  await setState(env, m.id, m.state, now);
  return live;
}

/** `started`: it is booting now. `running`: it never stopped. `retry`: the provider can't start it yet (still
 * stopping, start limit reached, unreachable). `none`: not a cloud computer, or one that can't be started (its owner's
 * key was refused, or it failed): only its owner can change that, so nothing retries. */
export type WakeResult = "none" | "started" | "running" | "retry";

/** What a wake asked of the provider came to (`cloud_computer_woken`): `started` (resumed), `restored` (started from
 * its backup), `refused` (only its owner can fix what the provider turned down), `failed` (anything else). */
export type WokenOutcome = "started" | "restored" | "refused" | "failed";

/** What made the relay wake it (`cloud_computer_woken`'s `cause`): a device connected, a mailbox message or routine
 * webhook arrived, an access request came in, the time its host asked for (its next routine), or a wake retried. */
export type WakeCause = "device" | "message" | "access" | "routine" | "retry";

/** A wake: `result` for the relay, and `woken` when it asked the provider to start the machine (what that came to). */
export interface Wake {
  result: WakeResult;
  woken?: { outcome: WokenOutcome; machine: CloudMachineRow };
}

/** Asked the provider to start `machine`, which came to `outcome`. */
const asked = (result: WakeResult, outcome: WokenOutcome, machine: CloudMachineRow): Wake => ({ result, woken: { outcome, machine } });
const failure = (o: { needsOwner: boolean }): WokenOutcome => (o.needsOwner ? "refused" : "failed");

/** Starts the sleeping cloud machine behind `computerId`. */
export async function wake(env: Env, computerId: string, now: number): Promise<Wake> {
  const m = await machineFor(env, computerId);
  if (!m) return { result: "none" };
  const d = drivers[m.provider];
  const live = await refresh(env, m, now);
  if (!live.ok) return { result: live.needsOwner ? "none" : "retry" };
  // Still stopping, or still awake while its stop takes effect (`refresh`).
  if (m.state === "stopping") return { result: "retry" };
  const state = live.value.state;
  if (state === "running") return { result: "running" };
  if (state === "starting") return { result: "started" };
  if (state !== "stopped") {
    console.error(JSON.stringify({ msg: "cloud computer can't wake", computerId, state }));
    return { result: "none" };
  }
  const resumed = d.machine(await providerSend(env, m.user_id, m.provider, d.wake(m.provider_id, crypto.randomUUID()), now));
  if (resumed.ok) {
    await setState(env, m.id, resumed.value.state, now);
    return asked("started", "started", m);
  }
  if (resumed.code === "backupNeeded" && m.backup) return fromBackup(env, m, now);
  console.log(JSON.stringify({ msg: "cloud computer resume refused", computerId, code: resumed.detail }));
  return asked(resumed.needsOwner ? "none" : "retry", failure(resumed), m);
}

/** The machine's own snapshot expired: a new one from its backup takes its place (same disk, new provider id). */
async function fromBackup(env: Env, m: CloudMachineRow, now: number): Promise<Wake> {
  const backup = drivers[m.provider].backup;
  if (!backup) return { result: "none" };
  const spec = { name: machineName(env, m.user_id, m.id), size: SIZE, group: GROUP, autoStopSeconds: autoStop(env) };
  const made = drivers[m.provider].machine(await providerSend(env, m.user_id, m.provider, backup.restore(spec, m.backup!, `${m.id}-${m.backup_at}`), now));
  if (!made.ok) {
    console.error(JSON.stringify({ msg: "cloud computer restore from backup failed", machine: m.id, code: made.detail }));
    return asked(made.retryable ? "retry" : "none", failure(made), m);
  }
  await env.DB.prepare("UPDATE cloud_machines SET provider_id = ?, state = ?, updated_at = ? WHERE id = ?")
    .bind(made.value.id, made.value.state, now, m.id)
    .run();
  return asked("started", "restored", m);
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

/** Deletes a machine's backup, if it has one (one already gone counts); the backup outlives the machine otherwise. */
export async function dropBackup(env: Env, m: CloudMachineRow): Promise<Outcome<null>> {
  const d = drivers[m.provider];
  if (!m.backup || !d.backup) return { ok: true, value: null };
  const gone = d.deleted(await providerSend(env, m.user_id, m.provider, d.backup.drop(m.backup)));
  if (gone.ok) await env.DB.prepare("UPDATE cloud_machines SET backup = NULL, backup_at = NULL WHERE id = ?").bind(m.id).run();
  return gone;
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

/** `refresh`, then `setUp` once it runs. A provider out of reach is logged, not thrown: the machine keeps its last state,
 * and one that throws doesn't hold up the rest. */
export async function settle(env: Env, m: CloudMachineRow, now: number): Promise<void> {
  try {
    const live = await refresh(env, m, now);
    if (live.ok) await setUp(env, m, live.value.state, now);
  } catch (e) {
    console.error(JSON.stringify({ msg: "cloud computer check failed", machine: m.id, error: String(e) }));
  }
}

/** Cron: machines still waiting for their setup, each on its own. */
export async function setUpPending(env: Env, now: number): Promise<void> {
  const { results } = await env.DB.prepare("SELECT * FROM cloud_machines WHERE setup_at IS NULL AND state NOT IN ('deleted', 'failed')").all<CloudMachineRow>();
  for (const m of results) await settle(env, m, now);
}

/**
 * The owner's Codync account is gone. Their cloud computers are on their own provider accounts, which keep them until
 * they delete them there: Codync stops each so it stops billing them, then forgets them and their keys. Throws while a
 * provider can't take the stop yet (busy, unreachable), so the account deletion is retried before the keys are gone.
 */
export async function forgetCloud(env: Env, userId: string, now: number): Promise<void> {
  for (const m of await machinesOf(env, userId)) {
    if (m.state === "stopped") continue;
    const d = drivers[m.provider];
    // Read first: a provider may have put it to sleep on its own, and a second sleep is refused there.
    const live = await liveMachine(env, m, now);
    const awake = live.ok && (live.value.state === "running" || live.value.state === "starting");
    const stopped = awake ? d.machine(await providerSend(env, userId, m.provider, d.sleep(m.provider_id, crypto.randomUUID()), now)) : live;
    if (!stopped.ok && stopped.retryable) throw new Error(`cloud computer ${m.id} couldn't be stopped yet: ${stopped.detail}`);
    if (!stopped.ok && stopped.code !== "notFound") console.error(JSON.stringify({ msg: "cloud computer left running", machine: m.id, code: stopped.detail }));
  }
  await env.DB.batch([
    markDeleted(env, "user_id = ?2 AND state != 'deleted'", userId, now),
    forgetCredential(env, userId, null),
  ]);
}
