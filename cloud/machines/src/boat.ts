// Boat (https://boat.dev, API v1 at https://boat.dev/api/v1): full Ubuntu VMs billed per second while
// they run. A stop snapshots the disk and frees the machine (kept 14 days); a resume restores it, on the
// same warm machine when it is recent, otherwise on a fresh one. Memory isn't kept across a cold resume.

import type { CreateSpec, HttpRequest, HttpResponse, Machine, MachineState, Outcome } from "./types";

export const BOAT_API_URL = "https://boat.dev/api/v1";

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

const sandbox = (id: string) => `/sandboxes/${encodeURIComponent(id)}`;

/** A sandbox for one of our users: none of the account's secrets inside (`noEnv`), no auto-stop unless
 * asked (Codync decides when it sleeps), and no monitoring (Boat keeps no command or process output). */
export function create(spec: CreateSpec, idempotencyKey: string): HttpRequest {
  return {
    method: "POST",
    path: "/sandboxes",
    headers: { "Idempotency-Key": idempotencyKey },
    body: {
      name: spec.name,
      type: SIZES[spec.size],
      ttlSeconds: spec.autoStopSeconds ?? null,
      noEnv: true,
      monitoring: false,
      files: spec.files.map((f) => ({ path: f.path, content: f.content })),
      setupScript: spec.setupScript,
    },
  };
}

export const get = (id: string): HttpRequest => ({ method: "GET", path: sandbox(id) });
export const stop = (id: string): HttpRequest => ({ method: "POST", path: `${sandbox(id)}/stop` });
export const resume = (id: string): HttpRequest => ({ method: "POST", path: `${sandbox(id)}/resume`, body: {} });
/** Boat asks for the id again in a header before it erases a sandbox and its snapshots. */
export const destroy = (id: string): HttpRequest => ({
  method: "DELETE",
  path: sandbox(id),
  headers: { "X-Ascii-Confirm-Delete": id },
});

/** Erases the disk history (Boat's automatic snapshots) of a running sandbox; it keeps running. Never on a
 * stopped one: its newest snapshot is all there is to resume from. */
export const deleteSnapshots = (id: string): HttpRequest => ({
  method: "DELETE",
  path: `${sandbox(id)}/snapshots`,
  headers: { "X-Ascii-Confirm-Delete": id },
});

/** A backup that never expires (a stopped sandbox's own snapshot goes after 14 days). `name` is the caller's,
 * 1–63 lowercase letters, digits or dashes; saving under it again replaces it. */
export const saveBackup = (id: string, name: string): HttpRequest => ({
  method: "POST",
  path: "/saved-snapshots",
  body: { sandboxId: id, name },
});

/** A backup outlives its sandbox, so deleting a machine for good also deletes its backup. */
export const deleteBackup = (name: string): HttpRequest => ({ method: "DELETE", path: `/saved-snapshots/${encodeURIComponent(name)}` });

/** A new sandbox from a backup, when the stopped sandbox's own snapshot has expired. */
export function fromBackup(spec: Pick<CreateSpec, "name" | "size" | "autoStopSeconds">, backup: string, idempotencyKey: string): HttpRequest {
  return {
    method: "POST",
    path: "/sandboxes",
    headers: { "Idempotency-Key": idempotencyKey },
    body: { name: spec.name, type: SIZES[spec.size], from: backup, ttlSeconds: spec.autoStopSeconds ?? null, noEnv: true, monitoring: false },
  };
}

/** Boat's answer when a stopped sandbox's own snapshot has expired: only a backup can bring it back. */
export const SNAPSHOT_EXPIRED = "snapshot_data_expired";

/** Reads an answer that carries no machine (snapshot deletes, backups): success or the failure. */
export function done(res: HttpResponse): Outcome<null> {
  return res.status >= 200 && res.status < 300 ? { ok: true, value: null } : failure(res);
}

/** Reads any sandbox answer (create, get, stop, resume): `{ ok, sandbox: { id, state } }`. */
export function machine(res: HttpResponse): Outcome<Machine> {
  if (res.status < 200 || res.status >= 300) return failure(res);
  const s = (res.body as { sandbox?: { id?: unknown; state?: unknown } } | null)?.sandbox;
  const state = typeof s?.state === "string" ? STATES[s.state] : undefined;
  if (typeof s?.id !== "string" || !state) {
    return { ok: false, code: "badResponse", message: "Boat answered without a sandbox", retryable: true };
  }
  return { ok: true, value: { provider: "boat", id: s.id, state } };
}

/** Reads a delete answer; a sandbox that is already gone counts as deleted. */
export function deleted(res: HttpResponse): Outcome<null> {
  if ((res.status >= 200 && res.status < 300) || res.status === 404) return { ok: true, value: null };
  return failure(res);
}

function failure(res: HttpResponse): Outcome<never> {
  const b = res.body as { code?: unknown; message?: unknown } | null;
  return {
    ok: false,
    code: typeof b?.code === "string" ? b.code : `http${res.status}`,
    message: typeof b?.message === "string" ? b.message : `Boat answered ${res.status}`,
    // No ready machine, a start limit, or Boat itself failing: worth another try later.
    retryable: res.status === 429 || res.status >= 500,
  };
}
