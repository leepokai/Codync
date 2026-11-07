// The provider key a user gives Codync Cloud (docs/plans/codync-cloud.md, "Bring your own cloud"): write-only. Saving
// checks it with the provider first and refuses one that can do more than Codync needs; the key is then sealed and
// never comes back out (`GET /v1/cloud-computers` shows its hint).

import { boat, type Outcome } from "../../machines/src";
import { credentialFor, credentialView, forgetCredential, problemText, type Provider, providerSend, saveCredential, sendWith } from "../cloud-credentials";
import { type CloudMachineRow, machineOf } from "../cloud-machines";
import { ApiError, audit, body, type Ctx, user } from "./common";
import { allowedUser, unreachable } from "./cloud-computers";

/** Boat keys: `boat_…` (`box_…` before the rename), letters, digits, `_` and `-`. */
const KEY = /^(boat|box)_[A-Za-z0-9_-]{8,200}$/;

const provider = (raw: string | undefined): Provider => {
  if (raw !== "boat") throw new ApiError("notFound");
  return raw;
};

/**
 * `PUT /v1/cloud-credentials/boat {key}`: checks the key with Boat, then keeps it (sealed) in place of any earlier one.
 * Refused (`keyRefused`, with what to fix) when Boat turns it down, when it can run the whole Boat account, or when a
 * cloud computer already exists that it can't reach.
 */
export async function saveCloudCredential(c: Ctx, [raw]: string[]) {
  const p = provider(raw);
  const u = await allowedUser(c);
  // Each save calls the provider: never a free way to test someone's keys.
  if (c.env.CLOUD_KEY_LIMITER && !(await c.env.CLOUD_KEY_LIMITER.limit({ key: u.userId })).success) throw new ApiError("rateLimited");
  const key = body(c).key;
  if (typeof key !== "string" || !KEY.test(key.trim())) throw new ApiError("keyRefused", problemText("format"));
  const clean = key.trim();
  const machine = await machineOf(c.env, u.userId);
  check(boat.done(await sendWith(c.env, clean, boat.checkKey())));
  const admin = boat.isAdmin(await sendWith(c.env, clean, boat.checkAdmin()));
  if (admin === null) throw unreachable();
  if (admin) {
    console.log(JSON.stringify({ msg: "cloud key not saved", userId: u.userId, code: "admin" }));
    throw new ApiError("keyRefused", problemText("admin"));
  }
  if (machine) await checkReaches(c, u.userId, clean, machine);
  const row = await saveCredential(c.env, u.userId, p, clean, c.now);
  await audit(c.env, u.userId, "cloudCredential.save", p, "ok", c.now).run();
  return { credential: credentialView(row) };
}

/** `out` must be a success; otherwise what Boat said (or `otherwise`) is what the user is told to fix. */
function check(out: Outcome<unknown>, otherwise?: string) {
  if (out.ok) return;
  if (out.retryable) throw unreachable();
  throw new ApiError("keyRefused", problemText(otherwise ?? out.code));
}

/**
 * A replacement must still reach the computer, or nothing could wake it or put it to sleep. One that finds it gone is
 * taken when the saved key can't reach it either (deleted at Boat, or that key revoked): nothing is lost, and the new
 * key lets the user delete it here.
 */
async function checkReaches(c: Ctx, userId: string, key: string, machine: CloudMachineRow) {
  const res = await sendWith(c.env, key, boat.get(machine.provider_id));
  if (res.status === 404) {
    const saved = boat.done(await providerSend(c.env, userId, boat.get(machine.provider_id), c.now));
    if (!saved.ok && !saved.retryable) return;
  }
  check(boat.done(res), "api_key_sandbox_forbidden");
}

/** `DELETE /v1/cloud-credentials/boat`: forgets the key. Not while a cloud computer runs on it: without the key
 * Codync couldn't put it to sleep or delete it, and it would keep billing its owner. */
export async function deleteCloudCredential(c: Ctx, [raw]: string[]) {
  const p = provider(raw);
  const u = await user(c);
  if (await machineOf(c.env, u.userId)) throw new ApiError("conflict", "Delete your cloud computer first: Codync needs the key to put it to sleep.");
  if (!(await credentialFor(c.env, u.userId, p))) return {};
  await c.env.DB.batch([forgetCredential(c.env, u.userId, p), audit(c.env, u.userId, "cloudCredential.delete", p, "ok", c.now)]);
  return {};
}
