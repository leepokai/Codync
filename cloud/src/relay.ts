// ComputerRelay: one Durable Object per computer (spec §7, §8.5).
//
// Presence, ciphertext forwarding between the host socket and device sockets, ACL admission, the
// offline mailbox (`relay-mailbox.ts`), the queue of public routine webhook deliveries (`relay-hooks.ts`), the
// cloud assistant's tool calls to the host (§7.9), and waking a sleeping Codync Cloud computer. It never parses
// channel frames and never stores chat. Reached only through requests the Worker builds itself
// (`X-Codync-Internal: 1`), never a client's own request.

import { DurableObject } from "cloudflare:workers";
import { fromB64url, NONCE_TTL_MS, randomId, utf8, verifyEd25519 } from "./auth";
import { wake } from "./cloud-machines";
import { HostCalls } from "./host-calls";
import type { Env } from "./index";
import { HookBox } from "./relay-hooks";
import { Mailbox } from "./relay-mailbox";
import {
  ALARM_MS,
  ASSISTANT_TIMEOUT_MS,
  errorResponse,
  MAX_DEVICE_SOCKETS,
  MAX_PAIRING_SOCKETS,
  PAIRING_MAX_MESSAGES,
  PAIRING_TTL_MS,
  PING,
  PONG,
  parseAcl,
  RATE_MAX_BYTES,
  RATE_MAX_MESSAGES,
  RATE_WINDOW_MS,
  send,
  SILENT_TIMEOUT_MS,
  validCloseCode,
  WAKE_DEBOUNCE_MS,
  WAKE_RETRY_MS,
  type Acl,
  type Attachment,
  type DeviceAttachment,
  type HostAttachment,
  type Msg,
} from "./relay-wire";

export class ComputerRelay extends DurableObject<Env> {
  private readonly sql: SqlStorage;
  /** Per-link rate windows (device → DO only). Lost on hibernation, which is acceptable (§7.6). */
  private readonly rates = new Map<string, { start: number; msgs: number; bytes: number }>();
  /** Sockets this instance retired. Hibernation drops it, so a socket still in it later means the DO never slept. */
  private readonly retiredHere = new WeakSet<WebSocket>();
  private readonly mailbox: Mailbox;
  private readonly hooks: HookBox;
  /** Cloud assistant calls waiting on the host. */
  private readonly calls = new HostCalls();

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(`
      CREATE TABLE IF NOT EXISTS meta(k TEXT PRIMARY KEY, v TEXT);
      CREATE TABLE IF NOT EXISTS nonces(kid TEXT, nonce TEXT, exp INTEGER, PRIMARY KEY(kid, nonce));
      CREATE TABLE IF NOT EXISTS blocked(dk TEXT PRIMARY KEY, grant_id TEXT, blocked_at INTEGER);
    `);
    this.mailbox = new Mailbox(this.sql);
    this.hooks = new HookBox(ctx.storage);
    ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair(PING, PONG));
  }

  // ---- entry points ----

  override async fetch(req: Request): Promise<Response> {
    if (req.headers.get("X-Codync-Internal") !== "1") return errorResponse(404, "notFound");
    const { pathname } = new URL(req.url);
    if (pathname === "/relay") return this.relay(req);
    if (pathname === "/internal/block" && req.method === "POST") return this.block(req);
    if (pathname === "/internal/hook" && req.method === "POST") return this.hook(req);
    if (pathname === "/internal/assistant" && req.method === "POST") return this.assistant(req);
    if (pathname === "/internal/sleep" && req.method === "POST") return this.sleep(req);
    if (pathname === "/internal/awake" && req.method === "POST") {
      await this.takeBackHost(Date.now());
      return Response.json({});
    }
    if (pathname === "/internal/changed" && req.method === "POST") {
      this.toHosts({ t: "cloud.changed" });
      // An access request: a sleeping cloud computer has to wake to answer it.
      const body = (await req.json().catch(() => null)) as { wake?: unknown } | null;
      if (body?.wake === true) await this.wakeHost(Date.now());
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
    for (const old of this.ctx.getWebSockets("host")) {
      const a = old.deserializeAttachment() as HostAttachment;
      if (!a.retired) await this.retire(old, a, now, 4009, "replaced");
    }
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
    const host = this.readyHost();
    if (host) send(host, JSON.stringify({ t: "open", link: a.link, dk, pair: a.pair }));
    else this.ctx.waitUntil(this.wakeHost(now));
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
      this.mailbox.drop(d.dk);
      dks.add(d.dk);
    }
    for (const [ws, a] of this.devices()) if (dks.has(a.dk)) this.safeClose(ws, 4003, "revoked");
    this.toHosts({ t: "cloud.changed" });
    return Response.json({});
  }

  // ---- routine webhooks (§7.8) ----

  private async hook(req: Request): Promise<Response> {
    const now = Date.now();
    const { res, queued } = await this.hooks.accept(req, now);
    if (queued) {
      const host = this.readyHost();
      if (host) this.hooks.deliver(host, now);
      else this.ctx.waitUntil(this.wakeHost(now));
      await this.ensureAlarm(now + ALARM_MS);
    }
    return res;
  }

  // ---- Codync Cloud sleep and wake ----

  /** `POST /internal/sleep {wakeAt}`: the cloud computer is about to stop; refused while a device is here. */
  private async sleep(req: Request): Promise<Response> {
    if (this.devices().some(([, d]) => !d.pair)) return errorResponse(409, "conflict");
    const b = (await req.json().catch(() => null)) as { wakeAt?: unknown } | null;
    const wakeAt = typeof b?.wakeAt === "number" && Number.isSafeInteger(b.wakeAt) ? b.wakeAt : null;
    this.setMeta("wakeAt", wakeAt === null ? "" : String(wakeAt));
    this.setMeta("sleepId", crypto.randomUUID());
    if (wakeAt !== null) await this.ensureAlarm(wakeAt);
    // Offline from now on: the machine stops within a minute, and its socket may never close cleanly. The host
    // stays connected until then; a wake that finds the machine still running once its stop had time to take
    // effect takes it back (`wakeHost`; before that, `wake` retries).
    for (const [ws, a] of this.hosts()) if (a.ready) ws.serializeAttachment({ ...a, ready: false });
    await this.hostOffline(Date.now());
    return Response.json({});
  }

  /** Starts this computer when it is a sleeping cloud computer, at most once a minute; retried while it can't.
   * Asked again within that minute, it tries once the minute is up: the computer may have gone back to sleep. */
  private async wakeHost(now: number): Promise<void> {
    const id = this.meta("computerId");
    if (!id || this.readyHost()) return;
    const woke = Number(this.meta("wokeAt") ?? 0);
    if (now - woke < WAKE_DEBOUNCE_MS) return this.wakeLater(woke + WAKE_DEBOUNCE_MS);
    this.setMeta("wokeAt", String(now));
    const sleepId = this.meta("sleepId");
    const result = await wake(this.env, id, now).catch((e) => {
      console.error("cloud computer wake failed", { computerId: id, error: String(e) });
      return "retry" as const;
    });
    // A host that asked to sleep while `wake` read its machine runs only until its stop takes effect: retried, as
    // a wake that found it `stopping` is.
    if (result === "running" && this.meta("sleepId") === sleepId) {
      await this.takeBackHost(now);
    } else if (result === "running" || result === "retry") {
      this.setMeta("wokeAt", "0");
      await this.wakeLater(now + WAKE_RETRY_MS);
    }
  }

  /** It never stopped (the stop failed): the host that asked to sleep is still connected and fine
   * (`POST /internal/awake`, or a wake that finds it running). */
  private async takeBackHost(now: number): Promise<void> {
    const [ws, a] = this.hosts().find(([, h]) => !h.ready) ?? [];
    if (ws && a) await this.markReady(ws, a, now);
  }

  /** Wakes it from the alarm at `at`, unless a wake is already due by then. */
  private async wakeLater(at: number): Promise<void> {
    const due = Number(this.meta("wakeAt") || 0);
    if (due && due <= at) return;
    this.setMeta("wakeAt", String(at));
    await this.ensureAlarm(at);
  }

  // ---- the cloud assistant (§7.9) ----

  /** `POST /internal/assistant {user, tool, args}`: the host's answer, as an `Answer`. */
  private async assistant(req: Request): Promise<Response> {
    const body = (await req.json().catch(() => null)) as { user?: unknown; tool?: unknown; args?: unknown } | null;
    if (typeof body?.user !== "string" || typeof body.tool !== "string") return errorResponse(400, "badRequest");
    const host = this.readyHost();
    if (!host) return Response.json({ ok: false, code: "offline" }, { status: 503 });
    const call = { t: "assistant.call", user: body.user, tool: body.tool, args: body.args ?? {} };
    return Response.json(await this.calls.ask(host, call, ASSISTANT_TIMEOUT_MS));
  }

  // ---- messages ----

  override async webSocketMessage(ws: WebSocket, message: string | ArrayBuffer): Promise<void> {
    const a = ws.deserializeAttachment() as Attachment;
    // It's alive after all, so closing it is quick.
    if (a.retired) return this.safeClose(ws, a.retired.code, a.retired.reason);
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
        return this.markReady(ws, a, now);
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
        this.mailbox.ack(msg, (dk, note) => this.toDevice(dk, note));
        return this.mailbox.deliver(ws, now);
      case "hooks":
        return this.hooks.replace(ws, msg);
      case "hook.ack": {
        const retry = this.hooks.ack(msg, now);
        if (retry !== null) await this.ensureAlarm(retry);
        return this.hooks.deliver(ws, now);
      }
      case "assistant.result":
        return this.calls.answered(msg);
      case "ping":
        return send(ws, PONG);
      default:
        return;
    }
  }

  /** The host is online: devices hear it and get their links, and queued mail and webhooks start flowing. */
  private async markReady(ws: WebSocket, a: HostAttachment, now: number): Promise<void> {
    ws.serializeAttachment({ ...a, ready: true });
    this.setMeta("lastSeenAt", String(now));
    this.setMeta("wakeAt", "");
    await this.recordPresence(true, now);
    const presence = JSON.stringify(this.presence());
    for (const [dws, d] of this.devices()) {
      send(dws, presence);
      send(ws, JSON.stringify({ t: "open", link: d.link, dk: d.dk, pair: d.pair }));
    }
    this.mailbox.deliver(ws, now);
    this.hooks.deliver(ws, now);
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
        if (msg.t === "mbox.cancel") return this.mailbox.cancel(ws, a.dk, msg);
        return this.mailbox.list(ws, a.dk);
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
    if (!this.mailbox.put(ws, a.dk, msg, now, !!this.readyHost())) return;
    this.ctx.waitUntil(this.wakeHost(now));
    await this.ensureAlarm(now + ALARM_MS);
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
    if (!a.retired) await this.gone(ws, a, Date.now());
  }

  /**
   * Lets go of a socket: nothing is sent to it any more and the bookkeeping runs now. It isn't closed yet: closing
   * a peer that's unreachable keeps the DO awake (billed) until TCP gives up, while a socket left alone doesn't.
   * The alarm closes it later if it speaks again, or if the DO never got to sleep (data queued for it may be
   * what keeps it awake, and only closing frees that).
   */
  private async retire(ws: WebSocket, a: Attachment, now: number, code: number, reason: string): Promise<void> {
    ws.serializeAttachment({ ...a, ...(a.role === "host" ? { ready: false } : {}), retired: { at: now, code, reason } });
    this.retiredHere.add(ws);
    await this.gone(ws, a, now);
  }

  /** Bookkeeping for a socket that closed or that the DO let go of. */
  private async gone(ws: WebSocket, a: Attachment, now: number): Promise<void> {
    if (a.role === "host") {
      if (a.ready) await this.hostOffline(now, ws);
    } else {
      this.rates.delete(a.link);
      send(this.readyHost(ws), JSON.stringify({ t: "close", link: a.link }));
    }
  }

  override async alarm(): Promise<void> {
    const now = Date.now();
    // Silent sockets, device ones too, are retired: whatever the DO sends a peer that stopped reading stays
    // queued and keeps the DO awake (billed as duration) until the connection dies, often hours later.
    for (const ws of this.ctx.getWebSockets()) {
      const a = ws.deserializeAttachment() as Attachment;
      const heard = Math.max(a.at, this.ctx.getWebSocketAutoResponseTimestamp(ws)?.getTime() ?? 0);
      if (a.retired) {
        if (heard > a.retired.at || this.retiredHere.has(ws)) this.safeClose(ws, a.retired.code, a.retired.reason);
      } else if (now - heard > SILENT_TIMEOUT_MS) {
        await this.retire(ws, a, now, 1011, "timeout");
      }
    }
    this.mailbox.expire(now, (dk, note) => this.toDevice(dk, note));
    this.hooks.expire(now);
    this.sql.exec("DELETE FROM nonces WHERE exp <= ?", now);
    const host = this.readyHost();
    if (host) this.hooks.deliver(host, now);
    const wakeAt = Number(this.meta("wakeAt") || 0);
    if (wakeAt && wakeAt <= now) {
      this.setMeta("wakeAt", "");
      await this.wakeHost(now);
    }

    const acl = this.acl();
    let next = now + ALARM_MS;
    for (const [ws, d] of this.devices()) {
      if (!d.pair) continue;
      const exp = Math.min(d.at + PAIRING_TTL_MS, acl?.offers.find((o) => o.id === d.offer)?.exp ?? 0);
      if (exp <= now) this.safeClose(ws, 4410, "pairingExpired");
      else next = Math.min(next, exp);
    }
    // A delivery waiting out its retry wakes the DO when it's due.
    const due = this.hooks.nextRetry(now);
    if (due !== null) next = Math.min(next, due);
    // A sleeping cloud computer's next routine, or a wake that has to be retried.
    const pendingWake = Number(this.meta("wakeAt") || 0);
    if (pendingWake) next = Math.min(next, pendingWake);
    const busy = this.ctx.getWebSockets().length > 0 || this.mailbox.any() || this.hooks.any() || pendingWake > 0;
    if (busy) await this.ctx.storage.setAlarm(next);
  }

  /** The host went away: devices see it offline, in-flight mail goes back to the queue. */
  private async hostOffline(now: number, exclude?: WebSocket): Promise<void> {
    this.setMeta("lastSeenAt", String(now));
    this.mailbox.requeue();
    this.hooks.requeue();
    this.calls.offline();
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

  private hosts(): [WebSocket, HostAttachment][] {
    return this.ctx
      .getWebSockets("host")
      .map((ws): [WebSocket, HostAttachment] => [ws, ws.deserializeAttachment() as HostAttachment])
      .filter(([, a]) => !a.retired);
  }

  private devices(): [WebSocket, DeviceAttachment][] {
    return this.ctx
      .getWebSockets("device")
      .map((ws): [WebSocket, DeviceAttachment] => [ws, ws.deserializeAttachment() as DeviceAttachment])
      .filter(([, a]) => !a.retired);
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
    for (const ws of this.ctx.getWebSockets("host")) if (!(ws.deserializeAttachment() as HostAttachment).retired) send(ws, text);
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
