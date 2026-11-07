// Codync Cloud runs on the user's own provider account (docs/plans/codync-cloud.md, "Bring your own cloud"): the key
// they paste once is sealed in D1 (`secrets.ts`) and opened only to call the provider. It is never returned and never
// logged: the apps see its hint (`boat_…9abc`) and, when the provider turns it down, what the owner has to fix.

import { boat, type HttpRequest, type HttpResponse, type Outcome } from "../machines/src";
import type { Env } from "./index";
import { open, seal } from "./secrets";

export type Provider = "boat";

/** Shown in the apps: users see which provider runs their computer and where. */
export const PROVIDERS = { boat: { name: "Boat", region: "EU" } } as const;

/** The Boat environment every cloud computer is made in: the key users make for Codync is limited to it, so it
 * reaches no other sandbox of theirs. */
export const GROUP = "codync";

/** All Codync does with the key: no commands, files, SSH or account settings. */
const ACTIONS = ["sandbox.create", "sandbox.read", "sandbox.stop", "sandbox.resume", "sandbox.delete", "snapshot.read", "snapshot.write"];

/** How a user makes that key, for the apps to show: from here, so it changes without an app release. A key is limited
 * to an environment by its id, not its name (Boat refuses the name with `api_key_sandbox_forbidden`). */
export const keySetup = (p: Provider) => ({
  provider: p,
  providerName: PROVIDERS[p].name,
  page: "https://boat.dev/dashboard",
  commands: [
    `boat env new ${GROUP}`,
    `boat env set ${GROUP} --safe-for-third-parties true`,
    `boat env info ${GROUP}   # its "id" goes after --env`,
    `boat api-key create codync --env <id> --ttl never --actions ${ACTIONS.join(",")}`,
  ].join("\n"),
});

export interface CredentialRow {
  user_id: string;
  provider: Provider;
  sealed: string;
  key_id: string;
  hint: string;
  /** The provider's code when it last turned the key down; null while it takes it. */
  problem: string | null;
  created_at: number;
  updated_at: number;
}

/** What the owner reads, by the provider's code. The apps translate these exact sentences. */
const PROBLEMS: Record<string, string> = {
  unauthorized: "Boat didn't accept this key. It may have been revoked.",
  http401: "Boat didn't accept this key. It may have been revoked.",
  noKey: "Add your Boat API key again.",
  api_key_expired: "This Boat key has expired. Make a new one.",
  api_key_action_forbidden: "This Boat key is missing an action Codync needs. Make it with the actions shown.",
  api_key_sandbox_forbidden: "This Boat key can't reach your cloud computer.",
  http404: "This Boat key can't reach your cloud computer.",
  unknown_environment: "Your Boat account has no codync environment. Create it first.",
  environment_forbidden: "This Boat key can't use the codync environment.",
  http402: "Your Boat account needs an active plan.",
  machine_class_plan_required: "Your Boat account needs an active plan.",
  admin: "This key can manage your whole Boat account. Make one limited to Codync instead.",
  format: "That doesn't look like a Boat API key.",
};

export const problemText = (code: string) => PROBLEMS[code] ?? "Boat refused this key.";

export const credentialFor = (env: Env, userId: string, provider: Provider = "boat") =>
  env.DB.prepare("SELECT * FROM cloud_credentials WHERE user_id = ? AND provider = ?").bind(userId, provider).first<CredentialRow>();

/** The apps' view: never the key itself. */
export const credentialView = (r: CredentialRow) => ({
  provider: r.provider,
  providerName: PROVIDERS[r.provider].name,
  hint: r.hint,
  problem: r.problem ? problemText(r.problem) : null,
  updatedAt: r.updated_at,
});

/** `boat_…9abc`: its kind and last four, as Boat lists keys. */
export const hint = (key: string) => `${key.slice(0, key.indexOf("_") + 1)}…${key.slice(-4)}`;

/** Sends `req` to the provider with `key`. */
export async function sendWith(env: Env, key: string, req: HttpRequest): Promise<HttpResponse> {
  const base = env.BOAT_API_URL ?? boat.BOAT_API_URL;
  const res = await fetch(`${base}${req.path}`, {
    method: req.method,
    headers: { ...req.headers, authorization: `Bearer ${key}`, "content-type": "application/json" },
    body: req.body === undefined ? undefined : JSON.stringify(req.body),
  });
  return { status: res.status, body: await res.json().catch(() => null) };
}

/** Problems about one action or the plan, which a read that works can't rule out: only a write that works clears them. */
const WRITE_PROBLEMS = new Set(["api_key_action_forbidden", "http402", "machine_class_plan_required"]);

/** The key's problem after `req` got `out`: a refusal only its owner can fix replaces it, a call that works clears it. */
function problemAfter(row: CredentialRow, req: HttpRequest, out: Outcome<null>): string | null {
  if (!out.ok) return out.needsOwner ? out.code : row.problem;
  return req.method === "GET" && row.problem && WRITE_PROBLEMS.has(row.problem) ? row.problem : null;
}

/**
 * Sends `req` with this user's own key. A refusal only they can fix (the key, its scope, their plan) is kept on their
 * key for the apps to show, and cleared by the next call that shows it fixed.
 */
export async function providerSend(env: Env, userId: string, req: HttpRequest, now = Date.now()): Promise<HttpResponse> {
  const row = await credentialFor(env, userId);
  const key = row && (await open(env, userId, row.provider, { sealed: row.sealed, keyId: row.key_id }));
  if (!row || !key) return { status: 401, body: { code: "noKey", message: "No usable Boat key" } };
  const res = await sendWith(env, key, req);
  const problem = problemAfter(row, req, boat.done(res));
  if (problem !== row.problem) {
    if (problem) console.log(JSON.stringify({ msg: "cloud key refused", provider: row.provider, hint: row.hint, code: problem }));
    // Only on the key this call went out with: one saved meanwhile starts clean.
    await env.DB.prepare("UPDATE cloud_credentials SET problem = ?, updated_at = ? WHERE user_id = ? AND provider = ? AND sealed = ?")
      .bind(problem, now, userId, row.provider, row.sealed)
      .run();
  }
  return res;
}

/** Stores `key` for this user, sealed, in place of any earlier one. */
export async function saveCredential(env: Env, userId: string, provider: Provider, key: string, now: number): Promise<CredentialRow> {
  const s = await seal(env, userId, provider, key);
  const row: CredentialRow = { user_id: userId, provider, sealed: s.sealed, key_id: s.keyId, hint: hint(key), problem: null, created_at: now, updated_at: now };
  await env.DB.prepare(
    `INSERT INTO cloud_credentials(user_id, provider, sealed, key_id, hint, problem, created_at, updated_at)
     VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6, ?6)
     ON CONFLICT(user_id, provider) DO UPDATE SET sealed = ?3, key_id = ?4, hint = ?5, problem = NULL, updated_at = ?6`,
  )
    .bind(userId, provider, row.sealed, row.key_id, row.hint, now)
    .run();
  return row;
}

/** Forgets the user's key for `provider` (null: every one), as a statement for the caller's batch. */
export const forgetCredential = (env: Env, userId: string, provider: Provider | null) =>
  env.DB.prepare("DELETE FROM cloud_credentials WHERE user_id = ?1 AND (?2 IS NULL OR provider = ?2)").bind(userId, provider);
