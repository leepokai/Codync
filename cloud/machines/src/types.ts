// The provider-neutral vocabulary. A provider module (a `Driver`) turns these into HTTP requests and reads the
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

export type ProviderId = "boat" | "instabox";

/** A machine's life as Codync sees it, whatever the provider calls it. `running` means usable: booted and ready. */
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

/** What a new machine gets once: its files, then its setup script, run in the background. */
export interface Setup {
  files: MachineFile[];
  setupScript: string;
}

export interface CreateSpec extends Setup {
  name: string;
  size: Size;
  /** Where the provider groups the machine, so a key limited to that group reaches only these machines (Boat: an
   * environment). Unset, or a provider without groups: its default. */
  group?: string;
  /** Lets the provider stop the machine on its own after this long; unset, only the caller stops it.
   * Boat's trial refuses machines without one (at most 2 h). Providers that pause idle machines themselves ignore it. */
  autoStopSeconds?: number;
}

/** A new machine without its files and setup script (a backup brings its own disk). */
export type MachineSpec = Omit<CreateSpec, keyof Setup>;

/** Why a provider said no, in Codync's words, whatever the provider named it. */
export type Failure =
  /** The key was refused: revoked, invalid, not a key. */
  | "keyRejected"
  | "keyExpired"
  /** The key lacks an action (a scope) Codync needs. */
  | "keyMissingAction"
  /** The key can't reach this machine. */
  | "keyCantReach"
  /** The account has no group the key is limited to (Boat: the codync environment). */
  | "noGroup"
  /** The key can't use that group. */
  | "groupForbidden"
  /** The account needs a (paid) plan for this. */
  | "planRequired"
  /** The account has as many machines as its plan allows. */
  | "machineLimit"
  /** The account's free machine used its hours for the month. */
  | "freeHoursUsed"
  | "notFound"
  /** A stopped machine's own disk expired: only a backup brings it back. */
  | "backupNeeded"
  /** Another change is under way, a start limit, a cooldown: later may work. */
  | "busy"
  /** The provider is down or out of room. */
  | "unavailable"
  | "badResponse"
  | "refused";

export type Outcome<T> =
  | { ok: true; value: T }
  | {
      ok: false;
      code: Failure;
      /** The provider's own name for it, for logs. */
      detail: string;
      message: string;
      retryable: boolean;
      /** Only the provider account's owner can fix it: the key was refused, lacks an action or a group, or the
       * account can't pay. Trying again won't help until they do. */
      needsOwner: boolean;
    };

/** The failures only the account's owner can fix. */
export const OWNER_FAILURES: ReadonlySet<Failure> = new Set([
  "keyRejected",
  "keyExpired",
  "keyMissingAction",
  "keyCantReach",
  "noGroup",
  "groupForbidden",
  "planRequired",
  "machineLimit",
  "freeHoursUsed",
]);

/**
 * One provider's API, in Codync's verbs. Every builder returns a request for the caller to send; every reader turns
 * the answer back into the vocabulary above. What differs between providers in kind, not just in wording, is a member
 * present or not (`setup`, `backup`, `deleteHistory`), so the caller decides from those and never from the provider's
 * name.
 */
export interface Driver {
  readonly id: ProviderId;
  /** The API's base URL; the caller may point it elsewhere (tests). */
  readonly url: string;
  /** What this provider's keys look like: anything else is turned down before a call. */
  readonly keyPattern: RegExp;
  /** Present when a stopped machine's own disk expires (`backupNeeded`): the caller saves a backup that doesn't,
   * `beforeDays` into its sleep, and creates a machine from it when needed. Absent: a stopped disk stays until
   * the machine is deleted. */
  readonly backup?: {
    beforeDays: number;
    /** `name`: 1–63 lowercase letters, digits or dashes; saving under it again replaces it. */
    save(id: string, name: string): HttpRequest;
    drop(name: string): HttpRequest;
    restore(spec: MachineSpec, name: string, idempotencyKey: string): HttpRequest;
  };

  create(spec: CreateSpec, idempotencyKey: string): HttpRequest;
  get(id: string): HttpRequest;
  /** Puts a running machine to sleep: billing for its compute stops, its disk stays. */
  sleep(id: string, idempotencyKey: string): HttpRequest;
  /** Starts a sleeping machine. */
  wake(id: string, idempotencyKey: string): HttpRequest;
  /** Erases the machine and what the provider keeps of it. */
  destroy(id: string, idempotencyKey: string): HttpRequest;
  /** Present when a machine takes nothing at create (its `files` and `setupScript` are ignored there): writes the
   * files and starts the script in the background once it is running. Absent: the create call takes both. */
  setup?(id: string, setup: Setup): HttpRequest;
  /** Erases the provider's automatic history of a running machine's earlier states; absent when it keeps none. */
  deleteHistory?(id: string): HttpRequest;
  /** A cheap read that only a working key gets through. */
  checkKey(): HttpRequest;
  /** Asks whether a key can run the whole account (more than Codync needs), from `checkKey`'s answer; null when
   * nothing more is needed to know it can't. `isAdmin` reads the answer. */
  checkAdmin(checked: HttpResponse): HttpRequest | null;

  /** Any machine answer (create, get, sleep, wake). */
  machine(res: HttpResponse): Outcome<Machine>;
  /** An answer that carries no machine. */
  done(res: HttpResponse): Outcome<null>;
  /** A delete's answer; a machine that is already gone counts as deleted. */
  deleted(res: HttpResponse): Outcome<null>;
  /** `true` for a key that can run the whole account, `false` for one kept out, `null` when the provider didn't say
   * (busy, failing): never a reason to take the key. */
  isAdmin(res: HttpResponse): boolean | null;
}
