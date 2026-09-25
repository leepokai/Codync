// ComputerRelay: one Durable Object per computer (spec §7, §8.5).
//
// Presence, ciphertext forwarding between the host socket and device sockets, ACL admission, and the
// offline mailbox (device → host). It never parses channel frames and never stores chat. Reached only
// through requests the Worker builds itself (`X-Codync-Internal: 1`), never a client's own request.

import { DurableObject } from "cloudflare:workers";
import { b64url, fromB64url, NONCE_TTL_MS, randomId, utf8, verifyEd25519 } from "./auth";
import type { Env } from "./index";

const MAX_DEVICE_SOCKETS = 16;
const MAX_PAIRING_SOCKETS = 2;
const PAIRING_TTL_MS = 120_000;
const PAIRING_MAX_MESSAGES = 16;
const RATE_WINDOW_MS = 10_000;
const RATE_MAX_MESSAGES = 200;
const RATE_MAX_BYTES = 8 * 1024 * 1024;
const MBOX_MAX_BLOB = 64 * 1024;
const MBOX_PER_DEVICE = 50;
const MBOX_PER_COMPUTER = 500;
const MBOX_MAX_BYTES = 8 * 1024 * 1024;
const MBOX_TTL_MS = 24 * 3600_000;
const HOST_TIMEOUT_MS = 90_000;
const ALARM_MS = 60_000;
/** epk(32) ‖ sig(64) ‖ ciphertext with at least its 16-byte tag (§6.4). */
const MBOX_MIN_BLOB = 32 + 64 + 16;
const FAIL_CODES = new Set(["unauthorized", "unknownBot", "invalid"]);

export const PING = '{"t":"ping"}';
export const PONG = '{"t":"pong"}';

interface HostAttachment {
  role: "host";
  kid: string;
  ready: boolean;
  at: number;
}
interface DeviceAttachment {
  role: "device";
  dk: string;
  link: string;
  pair: boolean;
  offer?: string;
  at: number;
  msgs: number;
}
type Attachment = HostAttachment | DeviceAttachment;

export interface Acl {
  v: 1;
  computerId: string;
  ver: number;
  devices: { dk: string; grant: string | null }[];
  offers: { id: string; exp: number }[];
}

type Msg = Record<string, unknown> & { t: string };

const errorResponse = (status: number, code: string) => Response.json({ error: { code } }, { status });

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
function send(ws: WebSocket | undefined, text: string): void {
  try {
    ws?.send(text);
  } catch {
    // Closed underneath us; its close handler does the bookkeeping.
  }
}

const validCloseCode = (c: unknown): c is number =>
  typeof c === "number" && Number.isInteger(c) && (c === 1000 || (c >= 3000 && c <= 4999));

export class ComputerRelay extends DurableObject<Env> {
  private readonly sql: SqlStorage;
  /** Per-link rate windows (device → DO only). Lost on hibernation, which is acceptable (§7.6). */
  private readonly rates = new Map<string, { start: number; msgs: number; bytes: number }>();

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(`
      CREATE TABLE IF NOT EXISTS meta(k TEXT PRIMARY KEY, v TEXT);
      CREATE TABLE IF NOT EXISTS mailbox(seq INTEGER PRIMARY KEY AUTOINCREMENT, dk TEXT, nonce TEXT, blob BLOB,
        size INTEGER, exp INTEGER, state TEXT, UNIQUE(dk, nonce));
      CREATE TABLE IF NOT EXISTS nonces(kid TEXT, nonce TEXT, exp INTEGER, PRIMARY KEY(kid, nonce));
      CREATE TABLE IF NOT EXISTS blocked(dk TEXT PRIMARY KEY, grant_id TEXT, blocked_at INTEGER);
    `);
    ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair(PING, PONG));
  }

  // ---- entry points ----

  override async fetch(req: Request): Promise<Response> {
    if (req.headers.get("X-Codync-Internal") !== "1") return errorResponse(404, "notFound");
    const { pathname } = new URL(req.url);
    if (pathname === "/relay") return this.relay(req);
    if (pathname === "/internal/block" && req.method === "POST") return this.block(req);
    if (pathname === "/internal/changed" && req.method === "POST") {
      this.toHosts({ t: "cloud.changed" });
      return Response.json({});
    }
    return errorResponse(404, "notFound");
  }

  private async relay(req: Request): Promise<Response> {
    const h = (name: string) => req.headers.get(name) ?? "";
    if (h("Upgrade").toLowerCase() !== "websocket") return errorResponse(426, "upgradeRequired");
    const role = h("X-Codync-Role");
    const key = h("X-Codync-Key");
    const nonce = h("X-Codync-Nonce");
    if (!fromB64url(key, 32) || !fromB64url(nonce, 16)) return errorResponse(400, "badRequest");
    const now = Date.now();
    if (this.sql.exec("SELECT 1 FROM nonces WHERE kid = ? AND nonce = ?", key, nonce).toArray().length) {
      return errorResponse(401, "badSignature");
    }
    this.sql.exec("INSERT INTO nonces(kid, nonce, exp) VALUES (?, ?, ?)", key, nonce, now + NONCE_TTL_MS);
    if (role === "host") return this.acceptHost(key, h("X-Codync-Computer"), now);
    if (role === "device") return this.acceptDevice(key, h("X-Codync-Pair"), now);
    return errorResponse(400, "badRequest");
  }

  private async acceptHost(kid: string, computerId: string, now: number): Promise<Response> {
    const signKey = this.meta("signKey");
    if (signKey === null) {
      this.setMeta("signKey", kid);
      this.setMeta("computerId", computerId);
    } else if (signKey !== kid) {
      return errorResponse(403, "forbidden");
    }
    // A new host connection replaces the old one (§7.2).
    let wasOnline = false;
    for (const old of this.ctx.getWebSockets("host")) {
      const a = old.deserializeAttachment() as HostAttachment;
      wasOnline ||= a.ready;
      old.serializeAttachment({ ...a, ready: false });
      this.safeClose(old, 4009, "replaced");
    }
    if (wasOnline) await this.hostOffline(now);
    const { 0: client, 1: server } = new WebSocketPair();
    this.ctx.acceptWebSocket(server, ["host"]);
    server.serializeAttachment({ role: "host", kid, ready: false, at: now } satisfies HostAttachment);
    await this.ensureAlarm(now + ALARM_MS);
    return new Response(null, { status: 101, webSocket: client });
  }

  private async acceptDevice(dk: string, offer: string, now: number): Promise<Response> {
    const acl = this.acl();
    if (!acl) return errorResponse(404, "unknownComputer");
    // A pairing needs the host's one-time code, so the host decides; a block only stops ACL access.
    if (!offer && this.sql.exec("SELECT 1 FROM blocked WHERE dk = ?", dk).toArray().length) return errorResponse(403, "forbidden");
    const sockets = this.devices();
    if (sockets.length >= MAX_DEVICE_SOCKETS) return errorResponse(429, "rateLimited");
    let deadline = now + ALARM_MS;
    if (offer) {
      const o = acl.offers.find((o) => o.id === offer && o.exp > now);
      if (!o) return errorResponse(403, "forbidden");
      if (sockets.filter(([, a]) => a.pair).length >= MAX_PAIRING_SOCKETS) return errorResponse(429, "rateLimited");
      deadline = Math.min(deadline, now + PAIRING_TTL_MS, o.exp);
    } else if (!acl.devices.some((d) => d.dk === dk)) {
      return errorResponse(403, "forbidden");
    }
    const { 0: client, 1: server } = new WebSocketPair();
    this.ctx.acceptWebSocket(server, ["device"]);
    const a: DeviceAttachment = { role: "device", dk, link: randomId("", 12), pair: !!offer, at: now, msgs: 0 };
    if (offer) a.offer = offer;
    server.serializeAttachment(a);
    send(server, JSON.stringify(this.presence()));
    send(this.readyHost(), JSON.stringify({ t: "open", link: a.link, dk, pair: a.pair }));
    await this.ensureAlarm(deadline);
    return new Response(null, { status: 101, webSocket: client });
  }

  /** `POST /internal/block {"devices":[{dk, grantId}]}` (§3.4). */
  private async block(req: Request): Promise<Response> {
    const body = (await req.json().catch(() => null)) as { devices?: { dk?: unknown; grantId?: unknown }[] } | null;
    if (!Array.isArray(body?.devices)) return errorResponse(400, "badRequest");
    const now = Date.now();
    const dks = new Set<string>();
    for (const d of body.devices) {
      if (typeof d?.dk !== "string" || !fromB64url(d.dk, 32)) continue;
      const grant = typeof d.grantId === "string" ? d.grantId : null;
      this.sql.exec(
        `INSERT INTO blocked(dk, grant_id, blocked_at) VALUES (?, ?, ?)
         ON CONFLICT(dk) DO UPDATE SET grant_id = excluded.grant_id, blocked_at = excluded.blocked_at`,
        d.dk,
        grant,
        now,
      );
      // Nothing it queued should reach the host any more.
      this.sql.exec("DELETE FROM mailbox WHERE dk = ? AND state = 'queued'", d.dk);
      dks.add(d.dk);
    }
    for (const [ws, a] of this.devices()) if (dks.has(a.dk)) this.safeClose(ws, 4003, "revoked");
    this.toHosts({ t: "cloud.changed" });
    return Response.json({});
  }

  // ---- messages ----

  override async webSocketMessage(ws: WebSocket, message: string | ArrayBuffer): Promise<void> {
    const a = ws.deserializeAttachment() as Attachment;
    if (typeof message !== "string") return this.safeClose(ws, 4002, "protocolError");
    let msg: Msg;
    try {
      msg = JSON.parse(message);
    } catch {
      return this.safeClose(ws, 4002, "protocolError");
    }
    if (!msg || typeof msg !== "object" || typeof msg.t !== "string") return this.safeClose(ws, 4002, "protocolError");
    if (a.role === "host") return this.onHost(ws, a, msg);
    return this.onDevice(ws, a, msg, message);
  }

  private async onHost(ws: WebSocket, a: HostAttachment, msg: Msg): Promise<void> {
    const now = Date.now();
    switch (msg.t) {
      case "acl":
        return this.onAcl(ws, a, msg, now);
      case "ready": {
        if (a.ready) return;
        if (!this.acl()) return send(ws, JSON.stringify({ t: "error", code: "badAcl", ver: 0 }));
        ws.serializeAttachment({ ...a, ready: true });
        this.setMeta("lastSeenAt", String(now));
        await this.recordPresence(true, now);
        const presence = JSON.stringify(this.presence());
        for (const [dws, d] of this.devices()) {
          send(dws, presence);
          send(ws, JSON.stringify({ t: "open", link: d.link, dk: d.dk, pair: d.pair }));
        }
        this.deliverNext(ws, now);
        return;
      }
      case "data": {
        if (!a.ready || !msg.m || typeof msg.m !== "object") return;
        send(this.deviceByLink(msg.link), JSON.stringify(msg.m));
        return;
      }
      case "close": {
        if (!a.ready) return;
        const d = this.deviceByLink(msg.link);
        if (d) this.safeClose(d, validCloseCode(msg.code) ? msg.code : 1000, typeof msg.reason === "string" ? msg.reason : "");
        return;
      }
      case "mbox.ack":
        return this.onAck(ws, msg, now);
      case "ping":
        return send(ws, PONG);
      default:
        return;
    }
  }

  private async onAcl(ws: WebSocket, a: HostAttachment, msg: Msg, now: number): Promise<void> {
    const stored = Number(this.meta("aclVer") ?? 0);
    const d = fromB64url(msg.d);
    const sig = fromB64url(msg.sig, 64);
    const pub = fromB64url(a.kid, 32);
    const acl = d && sig && pub && (await verifyEd25519(pub, sig, d)) ? parseAcl(d) : null;
    if (!acl || acl.computerId !== this.meta("computerId")) {
      return send(ws, JSON.stringify({ t: "error", code: "badAcl", ver: stored }));
    }
    // Re-read after the await: another ACL may have been stored meanwhile.
    const current = Number(this.meta("aclVer") ?? 0);
    if (acl.ver <= current) return send(ws, JSON.stringify({ t: "error", code: "staleVersion", ver: current }));
    this.setMeta("aclVer", String(acl.ver));
    this.setMeta("aclD", msg.d as string);
    this.setMeta("aclSig", msg.sig as string);
    // Unblock only when the host authorized the dk anew: a different grant (a new approval) or
    // none (a QR pairing). Still listed with the revoked grant = the host hasn't caught up (§7.3).
    for (const row of this.sql.exec<{ dk: string; grant_id: string | null }>("SELECT dk, grant_id FROM blocked").toArray()) {
      if (acl.devices.some((x) => x.dk === row.dk && x.grant !== row.grant_id)) {
        this.sql.exec("DELETE FROM blocked WHERE dk = ?", row.dk);
      }
    }
    for (const [dws, dev] of this.devices()) {
      if (dev.pair) {
        if (!acl.offers.some((o) => o.id === dev.offer && o.exp > now)) this.safeClose(dws, 4410, "pairingExpired");
      } else if (!acl.devices.some((x) => x.dk === dev.dk)) {
        this.safeClose(dws, 4003, "revoked");
      }
    }
    send(ws, JSON.stringify({ t: "acl.ok", ver: acl.ver }));
  }

  private onAck(ws: WebSocket, msg: Msg, now: number): void {
    const row = this.sql
      .exec<{ dk: string; nonce: string }>("SELECT dk, nonce FROM mailbox WHERE seq = ? AND state = 'delivering'", Number(msg.seq))
      .toArray()[0];
    if (row) {
      this.sql.exec("DELETE FROM mailbox WHERE seq = ?", Number(msg.seq));
      const note =
        msg.ok === true
          ? { t: "mbox.delivered", nonce: row.nonce }
          : { t: "mbox.failed", nonce: row.nonce, code: FAIL_CODES.has(msg.code as string) ? msg.code : "invalid" };
      this.toDevice(row.dk, note);
    }
    this.deliverNext(ws, now);
  }

  private async onDevice(ws: WebSocket, a: DeviceAttachment, msg: Msg, raw: string): Promise<void> {
    const now = Date.now();
    if (!this.charge(a.link, utf8(raw).length, now)) return this.safeClose(ws, 4008, "rateLimited");
    if (a.pair) {
      a.msgs += 1;
      if (a.msgs > PAIRING_MAX_MESSAGES) return this.safeClose(ws, 4008, "rateLimited");
      ws.serializeAttachment(a);
    }
    switch (msg.t) {
      case "hello":
      case "f": {
        const host = this.readyHost();
        if (host) send(host, JSON.stringify({ t: "data", link: a.link, m: msg }));
        else send(ws, JSON.stringify(this.presence()));
        return;
      }
      case "ping":
        return send(ws, PONG);
      case "mbox.put":
      case "mbox.cancel":
      case "mbox.list":
        if (a.pair) return send(ws, JSON.stringify({ t: "mbox.err", nonce: msg.nonce ?? null, code: "pairing" }));
        if (msg.t === "mbox.put") return this.mboxPut(ws, a, msg, now);
        if (msg.t === "mbox.cancel") return this.mboxCancel(ws, a, msg);
        return this.mboxList(ws, a);
      default:
        return this.safeClose(ws, 4002, "protocolError");
    }
  }

  /** Fixed 10 s window per device link: ≤ 200 messages and ≤ 8 MiB (§7.6). */
  private charge(link: string, bytes: number, now: number): boolean {
    let r = this.rates.get(link);
    if (!r || now - r.start >= RATE_WINDOW_MS) {
      r = { start: now, msgs: 0, bytes: 0 };
      this.rates.set(link, r);
    }
    r.msgs += 1;
    r.bytes += bytes;
    return r.msgs <= RATE_MAX_MESSAGES && r.bytes <= RATE_MAX_BYTES;
  }

  // ---- mailbox (§7.7) ----

  private async mboxPut(ws: WebSocket, a: DeviceAttachment, msg: Msg, now: number): Promise<void> {
    const nonce = msg.nonce;
    const err = (code: string) => send(ws, JSON.stringify({ t: "mbox.err", nonce: nonce ?? null, code }));
    if (typeof nonce !== "string" || !nonce || nonce.length > 128) return err("invalid");
    const existing = this.sql
      .exec<{ state: string }>("SELECT state FROM mailbox WHERE dk = ? AND nonce = ?", a.dk, nonce)
      .toArray()[0];
    if (existing) return send(ws, JSON.stringify({ t: "mbox.ok", nonce, state: existing.state }));
    if (this.readyHost()) return err("hostOnline");
    if (typeof msg.d === "string" && msg.d.length > Math.ceil((MBOX_MAX_BLOB * 4) / 3)) return err("tooLarge");
    const blob = fromB64url(msg.d);
    if (!blob || blob.length < MBOX_MIN_BLOB) return err("invalid");
    if (blob.length > MBOX_MAX_BLOB) return err("tooLarge");
    const exp = msg.exp === undefined ? now + MBOX_TTL_MS : msg.exp;
    if (typeof exp !== "number" || !Number.isSafeInteger(exp) || exp <= now || exp > now + MBOX_TTL_MS) return err("invalid");
    const mine = this.sql.exec<{ n: number }>("SELECT COUNT(*) AS n FROM mailbox WHERE dk = ?", a.dk).one().n;
    const all = this.sql.exec<{ n: number; b: number }>("SELECT COUNT(*) AS n, COALESCE(SUM(size), 0) AS b FROM mailbox").one();
    if (mine >= MBOX_PER_DEVICE || all.n >= MBOX_PER_COMPUTER || all.b + blob.length > MBOX_MAX_BYTES) return err("full");
    this.sql.exec(
      "INSERT INTO mailbox(dk, nonce, blob, size, exp, state) VALUES (?, ?, ?, ?, ?, 'queued')",
      a.dk,
      nonce,
      blob.slice().buffer,
      blob.length,
      exp,
    );
    send(ws, JSON.stringify({ t: "mbox.ok", nonce, state: "queued" }));
    await this.ensureAlarm(now + ALARM_MS);
  }

  private mboxCancel(ws: WebSocket, a: DeviceAttachment, msg: Msg): void {
    const nonce = typeof msg.nonce === "string" ? msg.nonce : "";
    const row = this.sql
      .exec<{ state: string }>("SELECT state FROM mailbox WHERE dk = ? AND nonce = ?", a.dk, nonce)
      .toArray()[0];
    if (row?.state === "queued") {
      this.sql.exec("DELETE FROM mailbox WHERE dk = ? AND nonce = ?", a.dk, nonce);
      return send(ws, JSON.stringify({ t: "mbox.cancelled", nonce }));
    }
    send(ws, JSON.stringify({ t: "mbox.gone", nonce, state: row ? "delivering" : "unknown" }));
  }

  private mboxList(ws: WebSocket, a: DeviceAttachment): void {
    const items = this.sql
      .exec<{ nonce: string; exp: number; state: string }>("SELECT nonce, exp, state FROM mailbox WHERE dk = ? ORDER BY seq", a.dk)
      .toArray();
    send(ws, JSON.stringify({ t: "mbox.items", items }));
  }

  /** One item in flight at a time, in `seq` order; the next goes out after the host's ack. */
  private deliverNext(host: WebSocket, now: number): void {
    const a = host.deserializeAttachment() as HostAttachment;
    if (!a.ready) return;
    if (this.sql.exec("SELECT 1 FROM mailbox WHERE state = 'delivering'").toArray().length) return;
    const row = this.sql
      .exec<{ seq: number; dk: string; nonce: string; blob: ArrayBuffer; exp: number }>(
        "SELECT seq, dk, nonce, blob, exp FROM mailbox WHERE state = 'queued' AND exp > ? ORDER BY seq LIMIT 1",
        now,
      )
      .toArray()[0];
    if (!row) return;
    this.sql.exec("UPDATE mailbox SET state = 'delivering' WHERE seq = ?", row.seq);
    send(host, 
      JSON.stringify({ t: "mbox.item", seq: row.seq, from: row.dk, nonce: row.nonce, d: b64url(new Uint8Array(row.blob)), exp: row.exp }),
    );
  }

  // ---- lifecycle ----

  override async webSocketClose(ws: WebSocket): Promise<void> {
    await this.closed(ws);
  }

  override async webSocketError(ws: WebSocket): Promise<void> {
    await this.closed(ws);
  }

  private async closed(ws: WebSocket): Promise<void> {
    const a = ws.deserializeAttachment() as Attachment;
    this.safeClose(ws, 1000, "");
    if (a.role === "host") {
      if (a.ready) await this.hostOffline(Date.now(), ws);
    } else {
      this.rates.delete(a.link);
      send(this.readyHost(ws), JSON.stringify({ t: "close", link: a.link }));
    }
  }

  override async alarm(): Promise<void> {
    const now = Date.now();
    for (const ws of this.ctx.getWebSockets("host")) {
      const a = ws.deserializeAttachment() as HostAttachment;
      const last = Math.max(a.at, this.ctx.getWebSocketAutoResponseTimestamp(ws)?.getTime() ?? 0);
      if (now - last > HOST_TIMEOUT_MS) {
        ws.serializeAttachment({ ...a, ready: false });
        this.safeClose(ws, 1011, "timeout");
        if (a.ready) await this.hostOffline(now, ws);
      }
    }
    for (const row of this.sql
      .exec<{ dk: string; nonce: string }>("SELECT dk, nonce FROM mailbox WHERE exp <= ?", now)
      .toArray()) {
      this.toDevice(row.dk, { t: "mbox.expired", nonce: row.nonce });
    }
    this.sql.exec("DELETE FROM mailbox WHERE exp <= ?", now);
    this.sql.exec("DELETE FROM nonces WHERE exp <= ?", now);

    const acl = this.acl();
    let next = now + ALARM_MS;
    for (const [ws, d] of this.devices()) {
      if (!d.pair) continue;
      const exp = Math.min(d.at + PAIRING_TTL_MS, acl?.offers.find((o) => o.id === d.offer)?.exp ?? 0);
      if (exp <= now) this.safeClose(ws, 4410, "pairingExpired");
      else next = Math.min(next, exp);
    }
    const busy =
      this.ctx.getWebSockets().length > 0 || this.sql.exec("SELECT 1 FROM mailbox LIMIT 1").toArray().length > 0;
    if (busy) await this.ctx.storage.setAlarm(next);
  }

  /** The host went away: devices see it offline, in-flight mail goes back to the queue. */
  private async hostOffline(now: number, exclude?: WebSocket): Promise<void> {
    this.setMeta("lastSeenAt", String(now));
    this.sql.exec("UPDATE mailbox SET state = 'queued' WHERE state = 'delivering'");
    if (!this.readyHost(exclude)) {
      const presence = JSON.stringify(this.presence(exclude));
      for (const [ws] of this.devices()) send(ws, presence);
    }
    await this.recordPresence(false, now);
  }

  private async recordPresence(online: boolean, now: number): Promise<void> {
    const id = this.meta("computerId");
    if (!id) return;
    try {
      await this.env.DB.prepare("UPDATE computers SET online = ?, last_seen_at = ? WHERE id = ?").bind(online ? 1 : 0, now, id).run();
    } catch (e) {
      console.error("presence write failed", { computerId: id, error: String(e) });
    }
  }

  private async ensureAlarm(at: number): Promise<void> {
    const current = await this.ctx.storage.getAlarm();
    if (current === null || current > at) await this.ctx.storage.setAlarm(at);
  }

  // ---- helpers ----

  /** The single definition of "host online" (§7.2): a host socket whose attachment is `ready`. */
  private readyHost(exclude?: WebSocket): WebSocket | undefined {
    return this.ctx
      .getWebSockets("host")
      .find((ws) => ws !== exclude && (ws.deserializeAttachment() as HostAttachment).ready);
  }

  private presence(exclude?: WebSocket) {
    const last = this.meta("lastSeenAt");
    return { t: "presence", online: !!this.readyHost(exclude), lastSeenAt: last === null ? null : Number(last) };
  }

  private devices(): [WebSocket, DeviceAttachment][] {
    return this.ctx.getWebSockets("device").map((ws) => [ws, ws.deserializeAttachment() as DeviceAttachment]);
  }

  private deviceByLink(link: unknown): WebSocket | undefined {
    return this.devices().find(([, a]) => a.link === link)?.[0];
  }

  private toDevice(dk: string, msg: object): void {
    const text = JSON.stringify(msg);
    for (const [ws, a] of this.devices()) if (a.dk === dk && !a.pair) send(ws, text);
  }

  private toHosts(msg: object): void {
    const text = JSON.stringify(msg);
    for (const ws of this.ctx.getWebSockets("host")) send(ws, text);
  }

  private acl(): Acl | null {
    const d = this.meta("aclD");
    const bytes = d === null ? null : fromB64url(d);
    return bytes ? parseAcl(bytes) : null;
  }

  private meta(k: string): string | null {
    return this.sql.exec<{ v: string }>("SELECT v FROM meta WHERE k = ?", k).toArray()[0]?.v ?? null;
  }

  private setMeta(k: string, v: string): void {
    this.sql.exec("INSERT INTO meta(k, v) VALUES (?, ?) ON CONFLICT(k) DO UPDATE SET v = excluded.v", k, v);
  }

  private safeClose(ws: WebSocket, code: number, reason: string): void {
    try {
      ws.close(code, reason.slice(0, 40));
    } catch {
      // Already closing or closed.
    }
  }
}
