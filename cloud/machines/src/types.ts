// The provider-neutral vocabulary. A provider module turns these into HTTP requests and reads the
// answers back; it never sends anything itself and never sees a credential.

/** A request for the caller to send to a provider's API, relative to that provider's base URL.
 * The caller adds the base URL and its own `Authorization` header. */
export interface HttpRequest {
  method: "GET" | "POST" | "DELETE";
  path: string;
  headers?: Record<string, string>;
  body?: unknown;
}

/** What the caller got back: the status and the parsed JSON body (or `null`). */
export interface HttpResponse {
  status: number;
  body: unknown;
}

export type ProviderId = "boat";

/** A machine's life as Codync sees it, whatever the provider calls it. */
export type MachineState = "starting" | "running" | "stopping" | "stopped" | "failed" | "deleted";

export interface Machine {
  provider: ProviderId;
  id: string;
  state: MachineState;
}

export type Size = "small" | "medium";

/** A file to write on the machine before it first starts (a path under the user's home). */
export interface MachineFile {
  path: string;
  content: string;
}

export interface CreateSpec {
  name: string;
  size: Size;
  /** Written before the machine is ready, at its first start only. */
  files: MachineFile[];
  /** Runs once, in the background, after the machine is first ready. */
  setupScript: string;
  /** Lets the provider stop the machine on its own after this long; unset, only the caller stops it.
   * Boat's trial refuses machines without one (at most 2 h). */
  autoStopSeconds?: number;
}

export type Outcome<T> =
  | { ok: true; value: T }
  | { ok: false; code: string; message: string; retryable: boolean };
