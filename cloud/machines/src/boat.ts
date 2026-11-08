// Boat (https://boat.dev, API v1 at https://boat.dev/api/v1): full Ubuntu VMs billed per second while
// they run. A stop snapshots the disk and frees the machine (kept 14 days); a resume restores it, on the
// same warm machine when it is recent, otherwise on a fresh one. Memory isn't kept across a cold resume.

import { type Driver, type Failure, type HttpRequest, type HttpResponse, type MachineSpec, type MachineState, type Outcome, OWNER_FAILURES } from "./types";

const SIZES = { small: "small", medium: "default" } as const;

const STATES: Record<string, MachineState> = {
  init: "starting",
  provisioning: "starting",
  provisioned: "starting",
  cloning: "starting",
  ready: "running",
  idle: "running",
  running: "running",
  archiving: "stopping",
  archived: "stopped",
  error: "failed",
  cancelled: "deleted",
};

/** Boat's codes, in Codync's words; any other `api_key_…` code is a refused key, and the status decides the rest. */
const FAILURES: Record<string, Failure> = {
  unauthorized: "keyRejected",
  api_key_expired: "keyExpired",
  api_key_action_forbidden: "keyMissingAction",
  api_key_sandbox_forbidden: "keyCantReach",
  unknown_environment: "noGroup",
  environment_forbidden: "groupForbidden",
  machine_class_plan_required: "planRequired",
  snapshot_data_expired: "backupNeeded",
};

const BY_STATUS: Partial<Record<number, Failure>> = { 401: "keyRejected", 404: "notFound", 429: "busy" };

const sandbox = (id: string) => `/sandboxes/${encodeURIComponent(id)}`;

/** Boat asks for the id again in a header before it erases a sandbox or its snapshots. */
const confirm = (id: string) => ({ "X-Ascii-Confirm-Delete": id });

/** A sandbox for one of our users: none of the account's secrets inside (`noEnv`), no auto-stop unless asked (Codync
 * decides when it sleeps), and no monitoring (Boat keeps no command or process output). `from`: a backup's name. */
function sandboxRequest(spec: MachineSpec, idempotencyKey: string, extra: object): HttpRequest {
  return {
    method: "POST",
    path: "/sandboxes",
    headers: { "Idempotency-Key": idempotencyKey },
    body: {
      name: spec.name,
      type: SIZES[spec.size],
      ...extra,
      ...(spec.group ? { environment: spec.group } : {}),
      ttlSeconds: spec.autoStopSeconds ?? null,
      noEnv: true,
      monitoring: false,
    },
  };
}

function codeFor(status: number, detail: string): Failure {
  if (status === 402) return "planRequired"; // whatever Boat names a 402, it's the plan
  if (FAILURES[detail]) return FAILURES[detail];
  if (detail.startsWith("api_key_")) return "keyRejected";
  return BY_STATUS[status] ?? (status >= 500 ? "unavailable" : "refused");
}

function failure(res: HttpResponse): Outcome<never> {
  const b = res.body as { code?: unknown; message?: unknown } | null;
  const detail = typeof b?.code === "string" ? b.code : `http${res.status}`;
  const code = codeFor(res.status, detail);
  return {
    ok: false,
    code,
    detail,
    message: typeof b?.message === "string" ? b.message : `Boat answered ${res.status}`,
    // No ready machine, a start limit, or Boat itself failing: worth another try later.
    retryable: res.status === 429 || res.status >= 500,
    needsOwner: OWNER_FAILURES.has(code),
  };
}

const ok = (res: HttpResponse) => res.status >= 200 && res.status < 300;

export const boat: Driver = {
  id: "boat",
  url: "https://boat.dev/api/v1",
  /** `boat_…` (`box_…` before the rename), letters, digits, `_` and `-`. */
  keyPattern: /^(boat|box)_[A-Za-z0-9_-]{8,200}$/,
  backup: {
    beforeDays: 10,
    save: (id, name) => ({ method: "POST", path: "/saved-snapshots", body: { sandboxId: id, name } }),
    /** A backup outlives its sandbox, so deleting a machine for good also deletes its backup. */
    drop: (name) => ({ method: "DELETE", path: `/saved-snapshots/${encodeURIComponent(name)}` }),
    restore: (spec, name, idempotencyKey) => sandboxRequest(spec, idempotencyKey, { from: name }),
  },

  create: (spec, idempotencyKey) =>
    sandboxRequest(spec, idempotencyKey, { files: spec.files.map((f) => ({ path: f.path, content: f.content })), setupScript: spec.setupScript }),
  get: (id) => ({ method: "GET", path: sandbox(id) }),
  sleep: (id) => ({ method: "POST", path: `${sandbox(id)}/stop` }),
  wake: (id) => ({ method: "POST", path: `${sandbox(id)}/resume`, body: {} }),
  destroy: (id) => ({ method: "DELETE", path: sandbox(id), headers: confirm(id) }),
  /** Never on a stopped sandbox: its newest snapshot is all there is to resume from. */
  deleteHistory: (id) => ({ method: "DELETE", path: `${sandbox(id)}/snapshots`, headers: confirm(id) }),
  /** Reads one sandbox at most: answers 200 when a key works and may read sandboxes. */
  checkKey: () => ({ method: "GET", path: "/sandboxes?limit=1" }),
  /** Only a key that can run the whole account (an admin key, or one made before Boat had scopes) may read its
   * webhooks. Nothing else needs this call. */
  checkAdmin: () => ({ method: "GET", path: "/webhooks" }),

  /** `{ ok, sandbox: { id, state } }`. */
  machine(res) {
    if (!ok(res)) return failure(res);
    const s = (res.body as { sandbox?: { id?: unknown; state?: unknown } } | null)?.sandbox;
    const state = typeof s?.state === "string" ? STATES[s.state] : undefined;
    if (typeof s?.id !== "string" || !state) {
      return { ok: false, code: "badResponse", detail: "badResponse", message: "Boat answered without a sandbox", retryable: true, needsOwner: false };
    }
    return { ok: true, value: { provider: "boat", id: s.id, state } };
  },
  done: (res) => (ok(res) ? { ok: true, value: null } : failure(res)),
  deleted: (res) => (ok(res) || res.status === 404 ? { ok: true, value: null } : failure(res)),
  /** A scoped refusal (or no such route for the key) is `false`. */
  isAdmin(res) {
    if (ok(res)) return true;
    const code = (res.body as { code?: unknown } | null)?.code;
    const scoped = (res.status === 401 || res.status === 403) && typeof code === "string" && code.startsWith("api_key_");
    return scoped || res.status === 404 ? false : null;
  },
};
