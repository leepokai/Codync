// The routine webhook queue in a computer's relay (§7.8): deliveries checked against the keys the host
// registered, stored until the host is online, delivered one at a time, retried with backoff on request.

import { b64url } from "./auth";
import { authorized, deliveryId, forwarded, HOOK_ID, HOOK_MAX_BODY, parseHooks } from "./hooks";
import {
  errorResponse,
  HOOK_MAX_BYTES,
  HOOK_PER_COMPUTER,
  HOOK_PER_HOOK,
  HOOK_RETRY_MAX_MS,
  HOOK_RETRY_MIN_MS,
  HOOK_TTL_MS,
  MAX_HOOKS,
  send,
  type HostAttachment,
  type Msg,
} from "./relay-wire";

export class HookBox {
  private readonly storage: DurableObjectStorage;

  constructor(storage: DurableObjectStorage) {
    this.storage = storage;
    this.sql.exec(`
      CREATE TABLE IF NOT EXISTS hooks(id TEXT PRIMARY KEY, key TEXT NOT NULL, enabled INTEGER NOT NULL);
      CREATE TABLE IF NOT EXISTS hookbox(seq INTEGER PRIMARY KEY AUTOINCREMENT, hook TEXT NOT NULL,
        delivery TEXT NOT NULL, headers TEXT NOT NULL, body BLOB NOT NULL, size INTEGER NOT NULL, at INTEGER NOT NULL,
        exp INTEGER NOT NULL, state TEXT NOT NULL, attempts INTEGER NOT NULL DEFAULT 0,
        not_before INTEGER NOT NULL DEFAULT 0, UNIQUE(hook, delivery));
    `);
  }

  private get sql(): SqlStorage {
    return this.storage.sql;
  }

  /**
   * `POST /internal/hook` with the sender's body and forwarded headers, `X-Codync-Hook: <id>`.
   * 401 for an unknown hook or a wrong key alike, so hook ids can't be probed. `queued` when a new delivery
   * was stored.
   */
  async accept(req: Request, now: number): Promise<{ res: Response; queued: boolean }> {
    const refuse = (res: Response) => ({ res, queued: false });
    const id = req.headers.get("X-Codync-Hook") ?? "";
    const body = new Uint8Array(await req.arrayBuffer());
    if (!HOOK_ID.test(id)) return refuse(errorResponse(400, "badRequest"));
    if (body.length > HOOK_MAX_BODY) return refuse(errorResponse(413, "tooLarge"));
    const hook = this.sql.exec<{ key: string; enabled: number }>("SELECT key, enabled FROM hooks WHERE id = ?", id).toArray()[0];
    if (!hook || !(await authorized(hook.key, req.headers, body))) return refuse(errorResponse(401, "unauthorized"));
    if (!hook.enabled) return refuse(errorResponse(409, "paused"));
    const delivery = deliveryId(req.headers);
    if (delivery === null) return refuse(errorResponse(400, "badDeliveryId"));
    const dup = this.sql.exec("SELECT 1 FROM hookbox WHERE hook = ? AND delivery = ?", id, delivery).toArray().length > 0;
    if (dup) return refuse(Response.json({ delivery, queued: true, duplicate: true }, { status: 202 }));
    const mine = this.sql.exec<{ n: number }>("SELECT COUNT(*) AS n FROM hookbox WHERE hook = ?", id).one().n;
    const all = this.sql.exec<{ n: number; b: number }>("SELECT COUNT(*) AS n, COALESCE(SUM(size), 0) AS b FROM hookbox").one();
    if (mine >= HOOK_PER_HOOK || all.n >= HOOK_PER_COMPUTER || all.b + body.length > HOOK_MAX_BYTES) {
      return refuse(Response.json({ error: { code: "full" } }, { status: 429, headers: { "Retry-After": "60" } }));
    }
    this.sql.exec(
      "INSERT INTO hookbox(hook, delivery, headers, body, size, at, exp, state) VALUES (?, ?, ?, ?, ?, ?, ?, 'queued')",
      id,
      delivery,
      JSON.stringify(forwarded(req.headers)),
      body.slice().buffer,
      body.length,
      now,
      now + HOOK_TTL_MS,
    );
    return { res: Response.json({ delivery, queued: true }, { status: 202 }), queued: true };
  }

  /** `{"t":"hooks","hooks":[{id,key,enabled}]}`: the host's whole current set replaces the stored one. */
  replace(ws: WebSocket, msg: Msg): void {
    const hooks = parseHooks(msg.hooks, MAX_HOOKS);
    if (!hooks) return send(ws, JSON.stringify({ t: "error", code: "badHooks" }));
    this.storage.transactionSync(() => {
      this.sql.exec("DELETE FROM hooks");
      for (const h of hooks) this.sql.exec("INSERT INTO hooks(id, key, enabled) VALUES (?, ?, ?)", h.id, h.key, h.enabled ? 1 : 0);
      // Deliveries for a removed hook can't run any more; in-flight ones finish with their ack.
      this.sql.exec("DELETE FROM hookbox WHERE state = 'queued' AND hook NOT IN (SELECT id FROM hooks)");
    });
    send(ws, JSON.stringify({ t: "hooks.ok", n: hooks.length }));
  }

  /** `{"t":"hook.ack","seq","ok","retry"?}`: done, refused for good, or try again later (returns when). */
  ack(msg: Msg, now: number): number | null {
    const row = this.sql
      .exec<{ attempts: number }>("SELECT attempts FROM hookbox WHERE seq = ? AND state = 'delivering'", Number(msg.seq))
      .toArray()[0];
    if (!row) return null;
    if (msg.ok !== true && msg.retry === true) {
      const wait = Math.min(HOOK_RETRY_MIN_MS * 2 ** row.attempts, HOOK_RETRY_MAX_MS);
      this.sql.exec("UPDATE hookbox SET state = 'queued', attempts = attempts + 1, not_before = ? WHERE seq = ?", now + wait, Number(msg.seq));
      return now + wait;
    }
    this.sql.exec("DELETE FROM hookbox WHERE seq = ?", Number(msg.seq));
    return null;
  }

  /** One delivery in flight at a time, oldest first, skipping ones waiting out a retry. */
  deliver(host: WebSocket, now: number): void {
    const a = host.deserializeAttachment() as HostAttachment;
    if (!a.ready) return;
    if (this.sql.exec("SELECT 1 FROM hookbox WHERE state = 'delivering'").toArray().length) return;
    const row = this.sql
      .exec<{ seq: number; hook: string; delivery: string; headers: string; body: ArrayBuffer; at: number }>(
        "SELECT seq, hook, delivery, headers, body, at FROM hookbox WHERE state = 'queued' AND exp > ? AND not_before <= ? ORDER BY seq LIMIT 1",
        now,
        now,
      )
      .toArray()[0];
    if (!row) return;
    this.sql.exec("UPDATE hookbox SET state = 'delivering' WHERE seq = ?", row.seq);
    send(
      host,
      JSON.stringify({
        t: "hook.item",
        seq: row.seq,
        hook: row.hook,
        delivery: row.delivery,
        headers: JSON.parse(row.headers),
        body: b64url(new Uint8Array(row.body)),
        at: row.at,
      }),
    );
  }

  expire(now: number): void {
    this.sql.exec("DELETE FROM hookbox WHERE exp <= ? AND state = 'queued'", now);
  }

  /** When the earliest delivery waiting out a retry is due, so the alarm wakes the DO then. */
  nextRetry(now: number): number | null {
    return this.sql
      .exec<{ t: number | null }>("SELECT MIN(not_before) AS t FROM hookbox WHERE state = 'queued' AND not_before > ?", now)
      .one().t;
  }

  /** The host went away: the delivery it didn't ack goes back to the queue. */
  requeue(): void {
    this.sql.exec("UPDATE hookbox SET state = 'queued' WHERE state = 'delivering'");
  }

  any(): boolean {
    return this.sql.exec("SELECT 1 FROM hookbox LIMIT 1").toArray().length > 0;
  }
}
