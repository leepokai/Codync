// InstaBox (https://instabox.sh, API at https://api.instabox.sh): Firecracker machines (Debian, systemd, a guest user
// with passwordless sudo and its home on the kept `/data` disk) billed per second while running. A pause keeps memory
// and processes and resumes in seconds; a stopped or paused machine keeps its disks until it is deleted. Every change
// answers 202 with an operation and needs an `Idempotency-Key`. Nothing can be put on a machine at create: Codync
// writes its files and starts its setup with `exec` once it runs. Keys reach every machine of their workspace.

import { type Driver, type Failure, type HttpRequest, type HttpResponse, type MachineState, type Outcome, OWNER_FAILURES, type Setup } from "./types";

const STATES: Record<string, MachineState> = {
  creating: "starting",
  starting: "starting",
  resuming: "starting",
  running: "running",
  stopping: "stopping",
  suspending: "stopping",
  stopped: "stopped",
  suspended: "stopped",
  deleting: "deleted",
  deleted: "deleted",
  error: "failed",
};

/** Where an accepted change leaves the machine, by the operation's type. */
const OPERATIONS: Record<string, MachineState> = {
  create: "starting",
  start: "starting",
  resume: "starting",
  restore: "starting",
  stop: "stopping",
  suspend: "stopping",
  delete: "deleted",
};

/** InstaBox's codes, in Codync's words; the status decides the rest. */
const FAILURES: Record<string, Failure> = {
  expired_key: "keyExpired",
  api_key_expired: "keyExpired",
  missing_scope: "keyMissingAction",
  insufficient_scope: "keyMissingAction",
  permission_denied: "keyMissingAction",
  template_not_allowed: "planRequired",
  spec_not_allowed: "planRequired",
  credit_exhausted: "freeHoursUsed",
  quota_exceeded: "machineLimit",
  operation_conflict: "busy",
  machine_cooldown: "busy",
  rate_limited: "busy",
  concurrency_limit: "busy",
  no_capacity: "unavailable",
  host_unavailable: "unavailable",
  not_found: "notFound",
};

/** Codes InstaBox marks retryable only sometimes, in each answer's `retryable`: busy when it is, the plan's limit when it
 * isn't (`FAILURES` when the answer doesn't say). */
const LIMITS: ReadonlySet<string> = new Set(["concurrency_limit", "quota_exceeded"]);

const BY_STATUS: Partial<Record<number, Failure>> = { 401: "keyRejected", 402: "planRequired", 404: "notFound", 409: "busy", 429: "busy" };

/** The template every cloud computer starts from: the default, the one a free workspace may use too. */
const TEMPLATE = "agent-browser";

/** A free machine refuses any other idle pause; a paid one takes it too. Codync's host asks to sleep sooner. */
const IDLE_PAUSE_SECONDS = 1800;

const machinePath = (id: string) => `/v1/machines/${encodeURIComponent(id)}`;

const change = (id: string, verb: string, idempotencyKey: string): HttpRequest => ({
  method: "POST",
  path: `${machinePath(id)}/${verb}`,
  headers: { "Idempotency-Key": idempotencyKey },
});

const base64 = (text: string) => btoa(Array.from(new TextEncoder().encode(text), (b) => String.fromCharCode(b)).join(""));

/** A script for `bash -s` as the guest user: writes the files (private) under its home, then starts the setup script
 * detached, so the exec returns at once and the setup outlives it (its output in `~/.codync-setup.log`). */
export function setupCommand(setup: Setup): string {
  const lines = ["set -eu", "umask 077", "cd ~"];
  for (const f of setup.files) {
    const dir = f.path.includes("/") ? f.path.slice(0, f.path.lastIndexOf("/")) : ".";
    lines.push(`mkdir -p '${dir}'`, `printf %s '${base64(f.content)}' | base64 -d > '${f.path}'`);
  }
  lines.push(
    `printf %s '${base64(setup.setupScript)}' | base64 -d > .codync-setup.sh`,
    "setsid nohup bash .codync-setup.sh > .codync-setup.log 2>&1 < /dev/null &",
  );
  return `${lines.join("\n")}\n`;
}

function codeFor(status: number, detail: string, retryable: unknown): Failure {
  if (LIMITS.has(detail) && typeof retryable === "boolean") return retryable ? "busy" : "machineLimit";
  return FAILURES[detail] ?? BY_STATUS[status] ?? (status >= 500 ? "unavailable" : "refused");
}

function failure(res: HttpResponse): Outcome<never> {
  const e = (res.body as { error?: { code?: unknown; message?: unknown; retryable?: unknown } } | null)?.error;
  const detail = typeof e?.code === "string" ? e.code : `http${res.status}`;
  const code = codeFor(res.status, detail, e?.retryable);
  return {
    ok: false,
    code,
    detail,
    message: typeof e?.message === "string" ? e.message : `InstaBox answered ${res.status}`,
    retryable: code === "busy" || code === "unavailable" || e?.retryable === true,
    needsOwner: OWNER_FAILURES.has(code),
  };
}

const ok = (res: HttpResponse) => res.status >= 200 && res.status < 300;

const badResponse = (): Outcome<never> => ({
  ok: false,
  code: "badResponse",
  detail: "badResponse",
  message: "InstaBox answered without a machine",
  retryable: true,
  needsOwner: false,
});

export const instabox: Driver = {
  id: "instabox",
  url: "https://api.instabox.sh",
  /** `ibk_` and 52 of a–z, 2–7. */
  keyPattern: /^ibk_[a-z2-7]{52}$/,

  /** Without the account's Secrets or Vault: a machine that has them sends its HTTPS through InstaBox's proxy. */
  create: (spec, idempotencyKey) => ({
    method: "POST",
    path: "/v1/machines",
    headers: { "Idempotency-Key": idempotencyKey },
    body: {
      name: spec.name.toLowerCase(),
      templateId: TEMPLATE,
      vault: false,
      secrets: [],
      autoSuspendAfterSeconds: IDLE_PAUSE_SECONDS,
      metadata: { codync: "cloud" },
    },
  }),
  get: (id) => ({ method: "GET", path: machinePath(id) }),
  /** A pause: memory and processes kept, so the host carries on where it was. */
  sleep: (id, idempotencyKey) => change(id, "suspend", idempotencyKey),
  /** Starts a stopped or paused machine. */
  wake: (id, idempotencyKey) => change(id, "start", idempotencyKey),
  destroy: (id, idempotencyKey) => ({
    method: "DELETE",
    path: machinePath(id),
    headers: { "Idempotency-Key": idempotencyKey, "X-Confirm-Delete": id },
  }),
  setup: (id, setup) => ({
    method: "POST",
    path: `${machinePath(id)}/exec`,
    body: { cmd: "bash -s", stdin: setupCommand(setup), timeoutSeconds: 60, noHistory: true },
  }),
  /** Any working key may read who it belongs to. */
  checkKey: () => ({ method: "GET", path: "/v1/me" }),
  /** Only a workspace admin's key may list the workspace's invites. A personal workspace's keys can't be one. */
  checkAdmin(checked) {
    const b = checked.body as { orgId?: unknown; org?: { id?: unknown } } | null;
    const org = typeof b?.orgId === "string" ? b.orgId : typeof b?.org?.id === "string" ? b.org.id : null;
    return org ? { method: "GET", path: `/v1/orgs/${encodeURIComponent(org)}/invites` } : null;
  },

  /** A machine, or the operation a change started (`{ machineId, type, state }`). Running but not yet ready
   * (exec, files and SSH don't work yet) is still starting. */
  machine(res) {
    if (!ok(res)) return failure(res);
    const b = res.body as {
      id?: unknown;
      machineId?: unknown;
      type?: unknown;
      state?: unknown;
      runtime?: { ready?: unknown };
      error?: unknown;
    } | null;
    if (typeof b?.machineId === "string" && typeof b.type === "string") {
      if (b.state === "failed") return failure({ status: 500, body: { error: b.error } });
      const state = OPERATIONS[b.type];
      return state ? { ok: true, value: { provider: "instabox", id: b.machineId, state } } : badResponse();
    }
    const state = typeof b?.state === "string" ? STATES[b.state] : undefined;
    if (typeof b?.id !== "string" || !state) return badResponse();
    return { ok: true, value: { provider: "instabox", id: b.id, state: state === "running" && b.runtime?.ready !== true ? "starting" : state } };
  },
  /** An exec counts only when its command exited 0. */
  done(res) {
    if (!ok(res)) return failure(res);
    const exit = (res.body as { exitCode?: unknown } | null)?.exitCode;
    if (exit === undefined || exit === 0) return { ok: true, value: null };
    return { ok: false, code: "refused", detail: `exit${String(exit)}`, message: `The command exited ${String(exit)}`, retryable: true, needsOwner: false };
  },
  deleted: (res) => (ok(res) || res.status === 404 ? { ok: true, value: null } : failure(res)),
  /** `missing_scope` (or no such workspace for the key) is `false`. */
  isAdmin(res) {
    if (ok(res)) return true;
    const code = (res.body as { error?: { code?: unknown } } | null)?.error?.code;
    return code === "missing_scope" || res.status === 404 ? false : null;
  },
};
