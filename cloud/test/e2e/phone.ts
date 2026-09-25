// A fake phone for the cross-track e2e test (spec §12.1): a device key, the E2E channel over the relay
// or a direct `/channel` socket, inner RPC, subscriptions and the mailbox. Node only (global WebSocket).

import * as ref from "../ref.ts";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type Json = any;

export async function waitFor<T>(what: string, check: () => T | undefined | Promise<T | undefined>, ms = 15_000): Promise<T> {
  const deadline = Date.now() + ms;
  for (;;) {
    const v = await check();
    if (v !== undefined && v !== false) return v as T;
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}`);
    await new Promise((r) => setTimeout(r, 100));
  }
}

/** A WebSocket with a queue of parsed messages and the close code. */
export class Wire {
  readonly msgs: Json[] = [];
  close?: { code: number; reason: string };
  onMessage?: (m: Json) => boolean; // true = consumed
  readonly opened: Promise<void>;

  readonly ws: WebSocket;

  constructor(ws: WebSocket) {
    this.ws = ws;
    this.opened = new Promise((resolve, reject) => {
      ws.addEventListener("open", () => resolve());
      ws.addEventListener("error", () => reject(new Error(`WebSocket to ${ws.url} failed`)));
    });
    ws.addEventListener("message", (e) => {
      const m = JSON.parse(String(e.data));
      if (!this.onMessage?.(m)) this.msgs.push(m);
    });
    ws.addEventListener("close", (e) => {
      this.close = { code: e.code, reason: e.reason };
    });
  }

  static connect(url: string, key?: ref.SignKey): Wire {
    const u = new URL(url);
    const headers: Record<string, string> = {};
    if (key) headers["Codync-Sig"] = ref.signRequest(key, { method: "GET", authority: u.host, pathAndQuery: u.pathname + u.search });
    // undici's WebSocket takes headers in its init object (not in the DOM typings).
    return new Wire(new WebSocket(url, { headers } as unknown as string[]));
  }

  send(m: unknown) {
    this.ws.send(typeof m === "string" ? m : JSON.stringify(m));
  }

  next(pred: (m: Json) => boolean, what: string, ms = 15_000): Promise<Json> {
    return waitFor(what, () => {
      const i = this.msgs.findIndex(pred);
      if (i >= 0) return this.msgs.splice(i, 1)[0];
      if (this.close) throw new Error(`socket closed (${this.close.code} ${this.close.reason}) while waiting for ${what}`);
      return undefined;
    }, ms);
  }

  closed(ms = 15_000): Promise<number> {
    return waitFor("close", () => this.close?.code, ms);
  }
}

export interface PairingInfo {
  computerId: string;
  signKey: Uint8Array;
  boxKey: Uint8Array;
  code: string;
  urls: string[];
  cloud: string | null;
}

/** Parses a v3 `codync://pair?…` link (§4.1), refusing a mismatched computerId. */
export function parsePairing(link: string): PairingInfo {
  const u = new URL(link);
  const q = u.searchParams;
  if (u.protocol !== "codync:" || q.get("v") !== "3") throw new Error(`not a v3 pairing link: ${link}`);
  const signKey = ref.unb64url(q.get("sk") ?? "");
  if (ref.computerId(signKey) !== q.get("id")) throw new Error("pairing link id doesn't match sk");
  return {
    computerId: q.get("id")!,
    signKey,
    boxKey: ref.unb64url(q.get("bk") ?? ""),
    code: q.get("code") ?? "",
    urls: (q.get("urls") ?? "").split(",").filter(Boolean),
    cloud: q.get("cloud"),
  };
}

/** One E2E channel: handshake, then multiplexed inner RPC and subscriptions over encrypted frames. */
export class Channel {
  private codec!: ref.FrameCodec;
  private nextId = 1;
  private readonly pending = new Map<number, { resolve: (v: Json) => void; reject: (e: Error) => void }>();
  private readonly subs = new Map<number, (ev: Json) => void>();
  readonly events: Json[] = [];
  hello?: Json;

  readonly wire: Wire;
  private readonly relay: boolean;

  private constructor(wire: Wire, relay: boolean) {
    this.wire = wire;
    this.relay = relay;
  }

  /** Opens a channel. Over the relay it waits for `presence online:true` before `hello` (§7.5). */
  static async open(
    wire: Wire,
    device: ref.SignKey,
    hostSignPub: Uint8Array,
    o: { relay: boolean; pair?: boolean },
  ): Promise<Channel> {
    await wire.opened;
    const ch = new Channel(wire, o.relay);
    if (o.relay) await wire.next((m) => m.t === "presence" && m.online === true, "presence online");
    const d = ref.deviceHello(device, hostSignPub, { pair: o.pair });
    wire.send(d.hello);
    const answer = await wire.next((m) => m.t === "welcome" || m.t === "reject", "welcome", 10_000);
    if (answer.t === "reject") throw new Error(`rejected: ${answer.code} ${answer.message ?? ""}`);
    const keys = d.finish(answer);
    ch.codec = new ref.FrameCodec(keys.d2h, keys.h2d);
    wire.onMessage = (m) => {
      if (m.t !== "f") return false;
      const inner = ch.codec.open(m);
      if (inner !== undefined) ch.dispatch(inner);
      return true;
    };
    return ch;
  }

  private dispatch(m: Json) {
    const p = this.pending.get(m.id);
    if (p && ("ok" in m || "err" in m)) {
      this.pending.delete(m.id);
      if ("ok" in m) p.resolve(m.ok);
      else p.reject(Object.assign(new Error(`${m.err.status} ${m.err.message}`), { status: m.err.status }));
      return;
    }
    if ("ev" in m) {
      this.events.push(m.ev);
      this.subs.get(m.id)?.(m.ev);
    }
  }

  private send(inner: unknown) {
    for (const f of this.codec.seal(inner)) this.wire.send(f);
  }

  call(method: string, body: unknown = null, ms = 15_000): Promise<Json> {
    const id = this.nextId++;
    const done = new Promise<Json>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      setTimeout(() => {
        if (this.pending.delete(id)) reject(new Error(`${method} timed out`));
      }, ms);
    });
    this.send({ id, m: method, b: body });
    return done;
  }

  subscribe(sub: "events" | "term", body: unknown, onEvent: (ev: Json) => void = () => {}): number {
    const id = this.nextId++;
    this.subs.set(id, onEvent);
    this.send({ id, sub, b: body });
    return id;
  }

  cancel(id: number) {
    this.send({ id, cancel: true });
  }

  get isRelay() {
    return this.relay;
  }
}

export class FakePhone {
  readonly key = ref.signKey();
  get dk() {
    return ref.b64url(this.key.pub);
  }

  relayUrl(cloud: string, computerId: string, pair?: string): string {
    const base = cloud.replace(/^http/, "ws").replace(/\/$/, "");
    return `${base}/v1/relay/device/${computerId}?v=1${pair ? `&pair=${pair}` : ""}`;
  }

  relayWire(cloud: string, computerId: string, pair?: string): Wire {
    return Wire.connect(this.relayUrl(cloud, computerId, pair), this.key);
  }

  async relay(cloud: string, computerId: string, hostSignPub: Uint8Array): Promise<Channel> {
    return Channel.open(this.relayWire(cloud, computerId), this.key, hostSignPub, { relay: true });
  }

  async direct(base: string, hostSignPub: Uint8Array): Promise<Channel> {
    const url = `${base.replace(/^http/, "ws").replace(/\/$/, "")}/channel?v=1`;
    return Channel.open(Wire.connect(url), this.key, hostSignPub, { relay: false });
  }

  /** §4.1 over the relay: pair on a pairing link, expect the host to close it with 4100. */
  async pair(p: PairingInfo, name = "E2E iPhone"): Promise<Json> {
    const wire = this.relayWire(p.cloud!, p.computerId, ref.offerId(ref.unb64url(p.code)));
    const ch = await Channel.open(wire, this.key, p.signKey, { relay: true, pair: true });
    const ok = await ch.call("pair", { code: p.code, name, platform: "ios" });
    const code = await wire.closed();
    if (code !== 4100) throw new Error(`pairing link closed with ${code}, expected 4100`);
    return ok;
  }

  /** Seals and queues a `send` for an offline host (§6.4). */
  mailboxPut(wire: Wire, p: { signKey: Uint8Array; boxKey: Uint8Array }, botId: string, text: string, clientNonce: string) {
    const inner = JSON.stringify({ m: "send", b: { botId, text, clientNonce }, ts: Date.now() });
    const blob = ref.sealMailbox(this.key, p.signKey, p.boxKey, clientNonce, inner);
    wire.send({ t: "mbox.put", nonce: clientNonce, d: ref.b64url(blob) });
  }
}
