// Codync Cloud runs on the user's own provider account (docs/plans/codync-cloud.md, "Bring your own cloud"): the key
// they paste once is sealed in D1 (`secrets.ts`) and opened only to call the provider. It is never returned and never
// logged: the apps see its hint (`boat_…9abc`) and, when the provider turns it down, what the owner has to fix.

import { drivers, type Failure, type HttpRequest, type HttpResponse, type Outcome, type ProviderId } from "../machines/src";
import type { Env } from "./index";
import { open, seal } from "./secrets";

export type Provider = ProviderId;

/** Why a key was turned down: the provider's refusal, or Codync's own (it can run the whole account, or it isn't one of
 * this provider's keys). */
export type KeyProblem = Failure | "admin" | "format";

/** The Boat environment every cloud computer is made in: the key users make for Codync is limited to it, so it
 * reaches no other sandbox of theirs. */
export const GROUP = "codync";

/** All Codync does with a Boat key: no commands, files, SSH or account settings. */
const BOAT_ACTIONS = ["sandbox.create", "sandbox.read", "sandbox.stop", "sandbox.resume", "sandbox.delete", "snapshot.read", "snapshot.write"];

/** All Codync does with an InstaBox key: its machines (exec included, which is how it installs the host), no snapshots,
 * Secrets or workspace settings. InstaBox can't limit a key to some machines. */
const INSTABOX_SCOPES = ["machines:read", "machines:write", "machines:delete"];

/** A provider's own free plan, shown beside its key (the apps translate `label` and `detail`). */
interface ProviderOffer {
  label: string;
  detail: string;
  url: string;
}

interface ProviderInfo {
  name: string;
  region: string;
  keyPrompt: string;
  setup: { page: string; commands: string };
  offer?: ProviderOffer;
}

/**
 * Every provider the apps offer, in this order, with what they show: where it runs, how to make the key, and its free
 * plan if it has one (worded here, so a new provider or new steps reach every app without a release). Boat limits a key
 * to an environment by its id, not its name (the name gets `api_key_sandbox_forbidden`).
 */
export const PROVIDERS: Record<Provider, ProviderInfo> = {
  boat: {
    name: "Boat",
    region: "EU",
    keyPrompt: "boat_…",
    setup: {
      page: "https://boat.dev/dashboard",
      commands: [
        `boat env new ${GROUP}`,
        `boat env set ${GROUP} --safe-for-third-parties true`,
        `boat env info ${GROUP}   # its "id" goes after --env`,
        `boat api-key create codync --env <id> --ttl never --actions ${BOAT_ACTIONS.join(",")}`,
      ].join("\n"),
    },
  },
  instabox: {
    name: "InstaBox",
    region: "US",
    keyPrompt: "ibk_…",
    setup: {
      page: "https://dashboard.instabox.sh/dashboard/api-keys",
      commands: [
        "npm install -g instabox",
        "instabox login",
        `instabox keys create --name codync --scope ${INSTABOX_SCOPES.join(",")}`,
        "# The key reaches every machine in this workspace",
      ].join("\n"),
    },
    // InstaBox's pricing page, 2026-11-09: the Free plan is $0 with one machine, 4 vCPU / 8 GiB, 20 GiB disk.
    offer: {
      label: "Get a free sandbox",
      detail: "InstaBox's free plan includes one machine (4 vCPU, 8 GiB).",
      url: "https://instabox.sh/pricing",
    },
  },
};

/** Whether a route or a body names a provider in `PROVIDERS`. */
export const isProvider = (raw: unknown): raw is Provider => typeof raw === "string" && Object.hasOwn(PROVIDERS, raw);

export interface CredentialRow {
  user_id: string;
  provider: Provider;
  sealed: string;
  key_id: string;
  hint: string;
  /** Why the provider last turned the key down; null while it takes it. */
  problem: KeyProblem | null;
  created_at: number;
  updated_at: number;
}

/** What the owner reads, by why the key was turned down. The apps translate these exact sentences; they show under
 * the provider's name, so only a provider's own words (Boat's environment) name it. */
const PROBLEMS: Partial<Record<KeyProblem, string>> = {
  keyRejected: "This key wasn't accepted. It may have been revoked.",
  keyExpired: "This key has expired. Make a new one.",
  keyMissingAction: "This key is missing a permission Codync needs. Make it as shown.",
  keyCantReach: "This key can't reach your cloud computer.",
  noGroup: "Your Boat account has no codync environment. Create it first.",
  groupForbidden: "This Boat key can't use the codync environment.",
  planRequired: "This account needs an active plan.",
  machineLimit: "This account has as many machines as its plan allows. Delete one there or upgrade.",
  freeHoursUsed: "This account used its free hours for the month.",
  admin: "This key can manage your whole account. Make one limited to Codync instead.",
  format: "That doesn't look like an API key from this provider.",
};

export const problemText = (code: KeyProblem) => PROBLEMS[code] ?? "The provider refused this key.";

export const credentialFor = (env: Env, userId: string, provider: Provider) =>
  env.DB.prepare("SELECT * FROM cloud_credentials WHERE user_id = ? AND provider = ?").bind(userId, provider).first<CredentialRow>();

/** The user's keys, one per provider they gave one. */
export const credentialsOf = async (env: Env, userId: string) =>
  (await env.DB.prepare("SELECT * FROM cloud_credentials WHERE user_id = ?").bind(userId).all<CredentialRow>()).results;

/** Problems with the key itself: only another key fixes them. The rest (its plan, its environment, an action) a create
 * may find fixed at the provider, which clears them. */
const KEY_PROBLEMS: ReadonlySet<KeyProblem> = new Set(["keyRejected", "keyExpired", "admin", "format"]);

/** The apps' view of a key: never the key itself. `blocking`: nothing can be created with it until it's replaced. */
export const credentialView = (r: CredentialRow) => ({
  hint: r.hint,
  problem: r.problem ? problemText(r.problem) : null,
  blocking: r.problem !== null && KEY_PROBLEMS.has(r.problem),
  updatedAt: r.updated_at,
});

/** Each provider as the apps list it, with the user's key for it, if any. */
export const providersView = (keys: CredentialRow[]) =>
  (Object.keys(PROVIDERS) as Provider[]).map((p) => {
    const key = keys.find((k) => k.provider === p);
    return { provider: p, ...PROVIDERS[p], credential: key ? credentialView(key) : null };
  });

/** `boat_…9abc`: its kind and last four, as the providers list keys. */
export const hint = (key: string) => `${key.slice(0, key.indexOf("_") + 1)}…${key.slice(-4)}`;

/** Sends `req` to `provider` with `key`. */
export async function sendWith(env: Env, provider: Provider, key: string, req: HttpRequest): Promise<HttpResponse> {
  const base = { boat: env.BOAT_API_URL, instabox: env.INSTABOX_API_URL }[provider] ?? drivers[provider].url;
  const res = await fetch(`${base}${req.path}`, {
    method: req.method,
    headers: { ...req.headers, authorization: `Bearer ${key}`, "content-type": "application/json" },
    body: req.body === undefined ? undefined : JSON.stringify(req.body),
  });
  return { status: res.status, body: await res.json().catch(() => null) };
}

/** Problems about one action or the plan, which a read that works can't rule out: only a write that works clears them. */
const WRITE_PROBLEMS: ReadonlySet<KeyProblem> = new Set(["keyMissingAction", "planRequired", "machineLimit", "freeHoursUsed"]);

/** The key's problem after `req` got `out`: a refusal only its owner can fix replaces it, a call that works clears it. */
function problemAfter(row: CredentialRow, req: HttpRequest, out: Outcome<null>): KeyProblem | null {
  if (!out.ok) return out.needsOwner ? out.code : row.problem;
  return req.method === "GET" && row.problem && WRITE_PROBLEMS.has(row.problem) ? row.problem : null;
}

/**
 * Sends `req` with this user's own key for `provider`. A refusal only they can fix (the key, its scope, their plan) is kept on their
 * key for the apps to show, and cleared by the next call that shows it fixed.
 */
export async function providerSend(env: Env, userId: string, provider: Provider, req: HttpRequest, now = Date.now()): Promise<HttpResponse> {
  const row = await credentialFor(env, userId, provider);
  const key = row && (await open(env, userId, row.provider, { sealed: row.sealed, keyId: row.key_id }));
  // Read by the provider's driver as a refused key.
  if (!row || !key) return { status: 401, body: null };
  const res = await sendWith(env, provider, key, req);
  const problem = problemAfter(row, req, drivers[provider].done(res));
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
