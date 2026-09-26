// Reference device/host crypto for the Codync relay protocol (spec §4–§7), built on @noble/*.
// Test-only: checked against docs/reference/fixtures/remote-relay-vectors.json, then used by the unit tests and the e2e harness.
// Runs in both workerd (vitest) and Node (e2e).

import { chacha20poly1305 } from "@noble/ciphers/chacha.js";
import { ed25519, x25519 } from "@noble/curves/ed25519.js";
import { expand, extract } from "@noble/hashes/hkdf.js";
import { sha256 } from "@noble/hashes/sha2.js";

// ---- encoding ----

export const enc = new TextEncoder();
export const dec = new TextDecoder();

export function b64url(bytes: Uint8Array): string {
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

export function unb64url(s: string): Uint8Array {
  return Uint8Array.from(atob(s.replace(/-/g, "+").replace(/_/g, "/")), (c) => c.charCodeAt(0));
}

export function concat(...parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

export const random = (n: number) => crypto.getRandomValues(new Uint8Array(n));

const u64be = (c: number) => {
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, BigInt(c));
  return out;
};

function dh(priv: Uint8Array, pub: Uint8Array): Uint8Array {
  const ss = x25519.getSharedSecret(priv, pub);
  if (ss.every((b) => b === 0)) throw new Error("all-zero shared secret");
  return ss;
}

// ---- identity ----

export const cidRaw = (hostSignPub: Uint8Array) => sha256(hostSignPub).subarray(0, 16);
export const computerId = (hostSignPub: Uint8Array) => b64url(cidRaw(hostSignPub));

export interface SignKey {
  seed: Uint8Array;
  pub: Uint8Array;
}

export function signKey(seed: Uint8Array = random(32)): SignKey {
  return { seed, pub: ed25519.getPublicKey(seed) };
}

export const sign = (key: SignKey, msg: Uint8Array) => ed25519.sign(msg, key.seed);
export const verify = (pub: Uint8Array, sig: Uint8Array, msg: Uint8Array) => ed25519.verify(sig, msg, pub);

export interface BoxKey {
  priv: Uint8Array;
  pub: Uint8Array;
}

export function boxKey(priv: Uint8Array = random(32)): BoxKey {
  return { priv, pub: x25519.getPublicKey(priv) };
}

// ---- Codync-Sig (§5) ----

export function sigCanonical(method: string, authority: string, pathAndQuery: string, ts: number, nonce: string, body: Uint8Array) {
  return ["codync-sig-v1", method.toUpperCase(), authority.toLowerCase(), pathAndQuery, String(ts), nonce, b64url(sha256(body))].join("\n");
}

/** The `Codync-Sig` header value. */
export function signRequest(
  key: SignKey,
  req: { method: string; authority: string; pathAndQuery: string; body?: Uint8Array; ts?: number; nonce?: string },
): string {
  const ts = req.ts ?? Date.now();
  const nonce = req.nonce ?? b64url(random(16));
  const canonical = sigCanonical(req.method, req.authority, req.pathAndQuery, ts, nonce, req.body ?? new Uint8Array());
  return `v=1,kid=${b64url(key.pub)},ts=${ts},nonce=${nonce},sig=${b64url(sign(key, enc.encode(canonical)))}`;
}

// ---- handshake (§6.2) ----

export const hs1Input = (cid: Uint8Array, dk: Uint8Array, ekD: Uint8Array, n: Uint8Array) =>
  concat(enc.encode("codync/hs1/v1"), cid, dk, ekD, n);

export const transcriptHash = (cid: Uint8Array, dk: Uint8Array, ekD: Uint8Array, n: Uint8Array, ekH: Uint8Array) =>
  sha256(concat(enc.encode("codync/hs2/v1"), cid, dk, ekD, n, ekH));

export interface ChannelKeys {
  d2h: Uint8Array;
  h2d: Uint8Array;
}

export function channelKeys(ekPriv: Uint8Array, peerEk: Uint8Array, th: Uint8Array): ChannelKeys & { ss: Uint8Array; prk: Uint8Array } {
  const ss = dh(ekPriv, peerEk);
  const prk = extract(sha256, ss, th);
  return {
    ss,
    prk,
    d2h: expand(sha256, prk, enc.encode("codync/d2h/v1"), 32),
    h2d: expand(sha256, prk, enc.encode("codync/h2d/v1"), 32),
  };
}

/** Device side: builds `hello`, then `finish(welcome)` verifies the host and derives the keys. */
export function deviceHello(device: SignKey, hostSignPub: Uint8Array, opts: { pair?: boolean; eph?: BoxKey; n?: Uint8Array } = {}) {
  const eph = opts.eph ?? boxKey();
  const n = opts.n ?? random(32);
  const cid = cidRaw(hostSignPub);
  const hello: Record<string, unknown> = {
    t: "hello",
    v: 1,
    dk: b64url(device.pub),
    ek: b64url(eph.pub),
    n: b64url(n),
    sig: b64url(sign(device, hs1Input(cid, device.pub, eph.pub, n))),
  };
  if (opts.pair) hello.pair = true;
  return {
    hello,
    finish(welcome: { ek: string; sig: string }): ChannelKeys {
      const ekH = unb64url(welcome.ek);
      const th = transcriptHash(cid, device.pub, eph.pub, n, ekH);
      if (!verify(hostSignPub, unb64url(welcome.sig), th)) throw new Error("welcome signature doesn't verify");
      return channelKeys(eph.priv, ekH, th);
    },
  };
}

/** Host side (for tests that play the host): verifies `hello` and answers `welcome`. */
export function hostWelcome(host: SignKey, hello: { dk: string; ek: string; n: string; sig: string }, eph = boxKey()) {
  const cid = cidRaw(host.pub);
  const dk = unb64url(hello.dk);
  const ekD = unb64url(hello.ek);
  const n = unb64url(hello.n);
  if (!verify(dk, unb64url(hello.sig), hs1Input(cid, dk, ekD, n))) throw new Error("hello signature doesn't verify");
  const th = transcriptHash(cid, dk, ekD, n, eph.pub);
  return {
    welcome: { t: "welcome", v: 1, ek: b64url(eph.pub), sig: b64url(sign(host, th)) },
    keys: channelKeys(eph.priv, ekD, th),
  };
}

// ---- frames (§6.3) ----

export const CHUNK = 256 * 1024;

const frameNonce = (c: number) => concat(new Uint8Array(4), u64be(c));
const frameAad = (c: number) => concat(enc.encode("codync/frame/v1"), u64be(c));

export function sealFrame(key: Uint8Array, c: number, final: boolean, chunk: Uint8Array): string {
  const plaintext = concat(new Uint8Array([final ? 0 : 1]), chunk);
  return b64url(chacha20poly1305(key, frameNonce(c), frameAad(c)).encrypt(plaintext));
}

export function openFrame(key: Uint8Array, c: number, d: string): { final: boolean; chunk: Uint8Array } {
  const pt = chacha20poly1305(key, frameNonce(c), frameAad(c)).decrypt(unb64url(d));
  if (pt[0] !== 0 && pt[0] !== 1) throw new Error("bad frame flag");
  return { final: pt[0] === 0, chunk: pt.subarray(1) };
}

/** One direction of a channel: counters, chunking and reassembly. */
export class FrameCodec {
  private sendC = 0;
  private recvC = 0;
  private parts: Uint8Array[] = [];
  private readonly sendKey: Uint8Array;
  private readonly recvKey: Uint8Array;
  private readonly maxMessage: number;
  constructor(sendKey: Uint8Array, recvKey: Uint8Array, maxMessage = 16 * 1024 * 1024) {
    this.sendKey = sendKey;
    this.recvKey = recvKey;
    this.maxMessage = maxMessage;
  }

  seal(message: unknown): { t: "f"; c: number; d: string }[] {
    const bytes = enc.encode(JSON.stringify(message));
    const out: { t: "f"; c: number; d: string }[] = [];
    for (let at = 0; at === 0 || at < bytes.length; at += CHUNK) {
      const chunk = bytes.subarray(at, at + CHUNK);
      const final = at + CHUNK >= bytes.length;
      out.push({ t: "f", c: this.sendC, d: sealFrame(this.sendKey, this.sendC, final, chunk) });
      this.sendC += 1;
    }
    return out;
  }

  /** Returns the parsed message once its last chunk arrives; throws on a counter gap or bad tag. */
  open(frame: { c: number; d: string }): unknown | undefined {
    if (frame.c !== this.recvC) throw new Error(`frame counter ${frame.c}, expected ${this.recvC}`);
    const { final, chunk } = openFrame(this.recvKey, frame.c, frame.d);
    this.recvC += 1;
    this.parts.push(chunk);
    if (this.parts.reduce((n, p) => n + p.length, 0) > this.maxMessage) throw new Error("message too large");
    if (!final) return undefined;
    const msg = JSON.parse(dec.decode(concat(...this.parts)));
    this.parts = [];
    return msg;
  }
}

// ---- mailbox (§6.4) ----

export function mailboxKey(ss: Uint8Array, cid: Uint8Array, dk: Uint8Array, epk: Uint8Array) {
  const prk = extract(sha256, ss, concat(enc.encode("codync/mbox/v1"), cid, dk, epk));
  return expand(sha256, prk, enc.encode("codync/mbox-key/v1"), 32);
}

/** Seals one mailbox item. A fresh ephemeral key every call (MUST): the AEAD nonce is fixed. */
export function sealMailbox(device: SignKey, hostSignPub: Uint8Array, hostBoxPub: Uint8Array, clientNonce: string, inner: string, eph = boxKey()): Uint8Array {
  const cid = cidRaw(hostSignPub);
  const key = mailboxKey(dh(eph.priv, hostBoxPub), cid, device.pub, eph.pub);
  const ct = chacha20poly1305(key, new Uint8Array(12), concat(device.pub, enc.encode(clientNonce))).encrypt(enc.encode(inner));
  const sig = sign(device, concat(enc.encode("codync/mbox/v1"), cid, eph.pub, ct));
  return concat(eph.pub, sig, ct);
}

export function openMailbox(hostSignPub: Uint8Array, hostBox: BoxKey, dk: Uint8Array, clientNonce: string, blob: Uint8Array): string {
  const cid = cidRaw(hostSignPub);
  const epk = blob.subarray(0, 32);
  const sig = blob.subarray(32, 96);
  const ct = blob.subarray(96);
  if (!verify(dk, sig, concat(enc.encode("codync/mbox/v1"), cid, epk, ct))) throw new Error("mailbox signature doesn't verify");
  const key = mailboxKey(dh(hostBox.priv, epk), cid, dk, epk);
  return dec.decode(chacha20poly1305(key, new Uint8Array(12), concat(dk, enc.encode(clientNonce))).decrypt(ct));
}

// ---- SAS, pairing offer, claim, ACL, push ----

export const sasCommit = (dk: Uint8Array, nD: Uint8Array) => sha256(concat(enc.encode("codync/sascommit/v1"), dk, nD));

export function sasCode(hostSignPub: Uint8Array, dk: Uint8Array, nD: Uint8Array, nH: Uint8Array): string {
  const h = sha256(concat(enc.encode("codync/sas/v2"), hostSignPub, dk, nD, nH));
  return String(new DataView(h.buffer, h.byteOffset).getUint32(0) % 1_000_000).padStart(6, "0");
}

export const offerId = (code: Uint8Array) => b64url(sha256(concat(enc.encode("codync/offer/v1"), code)).subarray(0, 16));

export const claimCanonical = (claimId: string, nonce: string, userId: string, computerId: string, boxKey: string) =>
  ["codync/claim/v1", claimId, nonce, userId, computerId, boxKey].join("\n");

export interface AclBody {
  v: 1;
  computerId: string;
  ver: number;
  devices: { dk: string; grant: string | null }[];
  offers: { id: string; exp: number }[];
}

/** The host's `acl` message: signs the exact JSON bytes it sends. */
export function aclMessage(host: SignKey, acl: AclBody | string) {
  const bytes = enc.encode(typeof acl === "string" ? acl : JSON.stringify(acl));
  return { t: "acl", d: b64url(bytes), sig: b64url(sign(host, bytes)) };
}

export function pushKey(ss: Uint8Array, cid: Uint8Array, pushPub: Uint8Array, epk: Uint8Array) {
  const prk = extract(sha256, ss, concat(enc.encode("codync/push/v1"), cid, pushPub, epk));
  return expand(sha256, prk, enc.encode("codync/push-key/v1"), 32);
}

export function sealPush(hostSignPub: Uint8Array, pushPub: Uint8Array, plaintext: string, eph = boxKey()): string {
  const cid = cidRaw(hostSignPub);
  const key = pushKey(dh(eph.priv, pushPub), cid, pushPub, eph.pub);
  return b64url(concat(eph.pub, chacha20poly1305(key, new Uint8Array(12), cid).encrypt(enc.encode(plaintext))));
}

export function openPush(hostSignPub: Uint8Array, push: BoxKey, sealed: string): string {
  const bytes = unb64url(sealed);
  const cid = cidRaw(hostSignPub);
  const epk = bytes.subarray(0, 32);
  const key = pushKey(dh(push.priv, epk), cid, push.pub, epk);
  return dec.decode(chacha20poly1305(key, new Uint8Array(12), cid).decrypt(bytes.subarray(32)));
}
