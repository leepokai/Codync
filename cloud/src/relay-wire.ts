// The relay's limits, socket attachments and wire helpers, shared by `ComputerRelay` and its tests.

import { fromB64url } from "./auth";

export const MAX_DEVICE_SOCKETS = 16;
export const MAX_PAIRING_SOCKETS = 2;
export const PAIRING_TTL_MS = 120_000;
export const PAIRING_MAX_MESSAGES = 16;
export const RATE_WINDOW_MS = 10_000;
export const RATE_MAX_MESSAGES = 200;
export const RATE_MAX_BYTES = 8 * 1024 * 1024;
export const MBOX_MAX_BLOB = 64 * 1024;
export const MBOX_PER_DEVICE = 50;
export const MBOX_PER_COMPUTER = 500;
export const MBOX_MAX_BYTES = 8 * 1024 * 1024;
export const MBOX_TTL_MS = 24 * 3600_000;
export const MAX_HOOKS = 1000;
export const HOOK_TTL_MS = 72 * 3600_000;
export const HOOK_PER_HOOK = 100;
export const HOOK_PER_COMPUTER = 1000;
export const HOOK_MAX_BYTES = 16 * 1024 * 1024;
export const HOOK_RETRY_MIN_MS = 30_000;
export const HOOK_RETRY_MAX_MS = 15 * 60_000;
/** The host answers a cloud assistant's call at once; past this it's too old to know them, or stuck. */
export const ASSISTANT_TIMEOUT_MS = 10_000;
/** No ping for this long (both sides ping every 30 s): the socket is dead and gets closed. */
export const SILENT_TIMEOUT_MS = 90_000;
export const ALARM_MS = 60_000;
/** epk(32) ‖ sig(64) ‖ ciphertext with at least its 16-byte tag (§6.4). */
export const MBOX_MIN_BLOB = 32 + 64 + 16;
export const FAIL_CODES = new Set(["unauthorized", "unknownBot", "invalid"]);

export const PING = '{"t":"ping"}';
export const PONG = '{"t":"pong"}';

export interface HostAttachment {
  role: "host";
  kid: string;
  ready: boolean;
  at: number;
  retired?: Retired;
}
export interface DeviceAttachment {
  role: "device";
  dk: string;
  link: string;
  pair: boolean;
  offer?: string;
  at: number;
  msgs: number;
  retired?: Retired;
}
export type Attachment = HostAttachment | DeviceAttachment;
/** A socket the DO let go of: nothing is sent to it, and it's closed with `code` once that's free (§7.6). */
export interface Retired {
  at: number;
  code: number;
  reason: string;
}

export interface Acl {
  v: 1;
  computerId: string;
  ver: number;
  devices: { dk: string; grant: string | null }[];
  offers: { id: string; exp: number }[];
}

export type Msg = Record<string, unknown> & { t: string };

export const errorResponse = (status: number, code: string) => Response.json({ error: { code } }, { status });

/** Parses and shape-checks a signed ACL's JSON; `null` when malformed. */
export function parseAcl(bytes: Uint8Array): Acl | null {
  let acl: unknown;
  try {
    acl = JSON.parse(new TextDecoder("utf-8", { fatal: true, ignoreBOM: false }).decode(bytes));
  } catch {
    return null;
  }
  const a = acl as Partial<Acl> | null;
  const ok =
    !!a &&
    a.v === 1 &&
    typeof a.computerId === "string" &&
    Number.isSafeInteger(a.ver) &&
    (a.ver ?? 0) >= 1 &&
    Array.isArray(a.devices) &&
    a.devices.every(
      (d) => d && fromB64url(d.dk, 32) !== null && (d.grant === null || (typeof d.grant === "string" && d.grant.length <= 64)),
    ) &&
    Array.isArray(a.offers) &&
    a.offers.every((o) => o && fromB64url(o.id, 16) !== null && Number.isSafeInteger(o.exp));
  return ok ? (a as Acl) : null;
}

/** Sends unless the socket is gone; a socket can be closing while it's still listed. */
export function send(ws: WebSocket | undefined, text: string): void {
  try {
    ws?.send(text);
  } catch {
    // Closed underneath us; its close handler does the bookkeeping.
  }
}

export const validCloseCode = (c: unknown): c is number =>
  typeof c === "number" && Number.isInteger(c) && (c === 1000 || (c >= 3000 && c <= 4999));
/** A sleeping cloud computer is asked to start at most this often. */
export const WAKE_DEBOUNCE_MS = 60_000;
/** When the provider can't start it yet (still stopping, start limit), ask again after this long. */
export const WAKE_RETRY_MS = 20_000;
