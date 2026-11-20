// Secrets users bring, sealed at rest (docs/plans/codync-cloud.md, "Bring your own cloud"): D1 keeps only ciphertext,
// and the keys that open it are the Worker secret `CLOUD_SECRET_KEYS`, `id:base64` entries separated by spaces. The
// first entry seals; any entry opens, so a key is rotated by putting a new one first. A row is sealed again only when
// its user saves their key again: an old entry stays while any row's `key_id` still names it.

import { b64url, concat, fromB64url, utf8 } from "./auth";
import type { Env } from "./index";

const ENTRY = /^([a-z0-9]{1,16}):([A-Za-z0-9+/]{43}=)$/;
const IV_BYTES = 12;

export interface Sealed {
  /** IV and ciphertext, base64url. */
  sealed: string;
  keyId: string;
}

/** Whether this Worker can keep secrets at all (Codync Cloud is off without it). */
export const canSeal = (env: Env) => !!env.CLOUD_SECRET_KEYS?.trim();

type Entry = [id: string, key: CryptoKey];

async function importEntry(entry: string): Promise<Entry> {
  const m = ENTRY.exec(entry);
  if (!m) throw new Error("Invalid CLOUD_SECRET_KEYS entry: expected id:base64 of 32 bytes");
  const raw = Uint8Array.from(atob(m[2]!), (ch) => ch.charCodeAt(0));
  return [m[1]!, await crypto.subtle.importKey("raw", raw, "AES-GCM", false, ["encrypt", "decrypt"])];
}

/** Every entry, the sealing one first. */
async function keys(env: Env): Promise<[Entry, ...Entry[]]> {
  const [first, ...rest] = (env.CLOUD_SECRET_KEYS ?? "").split(/\s+/).filter(Boolean);
  if (!first) throw new Error("CLOUD_SECRET_KEYS is not set");
  return Promise.all([importEntry(first), ...rest.map(importEntry)]);
}

/** Binds a sealed secret to its owner and purpose: a row copied to another account or provider doesn't open. */
const context = (owner: string, purpose: string) => utf8(`codync/secret/v1|${owner}|${purpose}`);

export async function seal(env: Env, owner: string, purpose: string, secret: string): Promise<Sealed> {
  const [[keyId, key]] = await keys(env);
  const iv = crypto.getRandomValues(new Uint8Array(IV_BYTES));
  const ct = await crypto.subtle.encrypt({ name: "AES-GCM", iv, additionalData: context(owner, purpose) }, key, utf8(secret));
  return { sealed: b64url(concat(iv, new Uint8Array(ct))), keyId };
}

/** The secret, or `null` when it can't be opened (its key was dropped, or the row was tampered with or moved). */
export async function open(env: Env, owner: string, purpose: string, s: Sealed): Promise<string | null> {
  const key = (await keys(env)).find(([id]) => id === s.keyId)?.[1];
  const bytes = fromB64url(s.sealed);
  if (!key || !bytes || bytes.length <= IV_BYTES) return null;
  try {
    const pt = await crypto.subtle.decrypt(
      { name: "AES-GCM", iv: bytes.subarray(0, IV_BYTES), additionalData: context(owner, purpose) },
      key,
      bytes.subarray(IV_BYTES),
    );
    return new TextDecoder().decode(pt);
  } catch {
    return null;
  }
}
