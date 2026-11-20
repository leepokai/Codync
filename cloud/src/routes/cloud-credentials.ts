// The provider key a user gives Codync Cloud (docs/plans/codync-cloud.md, "Bring your own cloud"): write-only. Saving
// checks it with the provider first and refuses one that can do more than Codync needs; the key is then sealed and
// never comes back out (`GET /v1/cloud-computers` shows its hint).

import { drivers, type Outcome } from "../../machines/src";
import { credentialFor, credentialView, forgetCredential, isProvider, type KeyProblem, problemText, type Provider, providerSend, saveCredential, sendWith } from "../cloud-credentials";
import { type CloudMachineRow, machinesOf } from "../cloud-machines";
import { ApiError, audit, body, type Ctx, user } from "./common";
import { cloudUser, unreachable } from "./cloud-computers";

const provider = (raw: string | undefined): Provider => {
  if (!isProvider(raw)) throw new ApiError("notFound");
  return raw;
};

/**
 * `PUT /v1/cloud-credentials/:provider {key}`: checks the key with the provider, then keeps it (sealed) in place of any
 * earlier one. Refused (`keyRefused`, with what to fix) when the provider turns it down, when it can run the whole
 * account, or when it can't reach one of the user's cloud computers on that provider.
 */
export async function saveCloudCredential(c: Ctx, [raw]: string[]) {
  const p = provider(raw);
  const u = await cloudUser(c);
  // Each save calls the provider: never a free way to test someone's keys.
  if (c.env.CLOUD_KEY_LIMITER && !(await c.env.CLOUD_KEY_LIMITER.limit({ key: u.userId })).success) throw new ApiError("rateLimited");
  const d = drivers[p];
  const key = body(c).key;
  if (typeof key !== "string" || !d.keyPattern.test(key.trim())) throw new ApiError("keyRefused", problemText("format"));
  const clean = key.trim();
  const machines = (await machinesOf(c.env, u.userId)).filter((m) => m.provider === p);
  const checked = await sendWith(c.env, p, clean, d.checkKey());
  check(d.done(checked));
  const probe = d.checkAdmin(checked);
  const admin = probe ? d.isAdmin(await sendWith(c.env, p, clean, probe)) : false;
  if (admin === null) throw unreachable();
  if (admin) {
    console.log(JSON.stringify({ msg: "cloud key not saved", userId: u.userId, code: "admin" }));
    throw new ApiError("keyRefused", problemText("admin"));
  }
  for (const machine of machines) await checkReaches(c, u.userId, clean, machine);
  const row = await saveCredential(c.env, u.userId, p, clean, c.now);
  await audit(c.env, u.userId, "cloudCredential.save", p, "ok", c.now).run();
  return { credential: credentialView(row) };
}

/** `out` must be a success; otherwise what the provider said (or `otherwise`) is what the user is told to fix. */
function check(out: Outcome<unknown>, otherwise?: KeyProblem) {
  if (out.ok) return;
  if (out.retryable) throw unreachable();
  throw new ApiError("keyRefused", problemText(otherwise ?? out.code));
}

/**
 * A replacement must still reach the computer, or nothing could wake it or put it to sleep. One that finds it gone is
 * taken when the saved key can't reach it either (deleted at the provider, or that key revoked): nothing is lost, and
 * the new key lets the user delete it here.
 */
async function checkReaches(c: Ctx, userId: string, key: string, machine: CloudMachineRow) {
  const d = drivers[machine.provider];
  const res = await sendWith(c.env, machine.provider, key, d.get(machine.provider_id));
  if (res.status === 404) {
    const saved = d.done(await providerSend(c.env, userId, machine.provider, d.get(machine.provider_id), c.now));
    if (!saved.ok && !saved.retryable) return;
  }
  check(d.done(res), "keyCantReach");
}

/** `DELETE /v1/cloud-credentials/:provider`: forgets the key. Not while a cloud computer runs on it: without the key
 * Codync couldn't put it to sleep or delete it, and it would keep billing its owner. */
export async function deleteCloudCredential(c: Ctx, [raw]: string[]) {
  const p = provider(raw);
  const u = await user(c);
  if ((await machinesOf(c.env, u.userId)).some((m) => m.provider === p)) {
    throw new ApiError("conflict", "Delete your cloud computers on it first: Codync needs the key to put them to sleep.");
  }
  if (!(await credentialFor(c.env, u.userId, p))) return {};
  await c.env.DB.batch([forgetCredential(c.env, u.userId, p), audit(c.env, u.userId, "cloudCredential.delete", p, "ok", c.now)]);
  return {};
}
