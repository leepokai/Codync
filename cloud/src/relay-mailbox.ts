// The offline mailbox in a computer's relay (§7.7): sealed device → host messages stored while the host is
// offline, delivered one at a time in order once it is back, and acknowledged to the device that sent them.

import { b64url, fromB64url } from "./auth";
import {
  FAIL_CODES,
  MBOX_MAX_BLOB,
  MBOX_MAX_BYTES,
  MBOX_MIN_BLOB,
  MBOX_PER_COMPUTER,
  MBOX_PER_DEVICE,
  MBOX_TTL_MS,
  send,
  type HostAttachment,
  type Msg,
} from "./relay-wire";

/** Sends a note to every regular (non-pairing) socket of device `dk`. */
export type ToDevice = (dk: string, msg: object) => void;

export class Mailbox {
  private readonly sql: SqlStorage;

  constructor(sql: SqlStorage) {
    this.sql = sql;
    this.sql.exec(`
      CREATE TABLE IF NOT EXISTS mailbox(seq INTEGER PRIMARY KEY AUTOINCREMENT, dk TEXT, nonce TEXT, blob BLOB,
        size INTEGER, exp INTEGER, state TEXT, UNIQUE(dk, nonce));
    `);
  }

  /** `mbox.put`; `true` when a new item was queued. */
  put(ws: WebSocket, dk: string, msg: Msg, now: number, hostOnline: boolean): boolean {
    const nonce = msg.nonce;
    const err = (code: string) => {
      send(ws, JSON.stringify({ t: "mbox.err", nonce: nonce ?? null, code }));
      return false;
    };
    if (typeof nonce !== "string" || !nonce || nonce.length > 128) return err("invalid");
    const existing = this.sql.exec<{ state: string }>("SELECT state FROM mailbox WHERE dk = ? AND nonce = ?", dk, nonce).toArray()[0];
    if (existing) {
      send(ws, JSON.stringify({ t: "mbox.ok", nonce, state: existing.state }));
      return false;
    }
    if (hostOnline) return err("hostOnline");
    if (typeof msg.d === "string" && msg.d.length > Math.ceil((MBOX_MAX_BLOB * 4) / 3)) return err("tooLarge");
    const blob = fromB64url(msg.d);
    if (!blob || blob.length < MBOX_MIN_BLOB) return err("invalid");
    if (blob.length > MBOX_MAX_BLOB) return err("tooLarge");
    const exp = msg.exp === undefined ? now + MBOX_TTL_MS : msg.exp;
    if (typeof exp !== "number" || !Number.isSafeInteger(exp) || exp <= now || exp > now + MBOX_TTL_MS) return err("invalid");
    const mine = this.sql.exec<{ n: number }>("SELECT COUNT(*) AS n FROM mailbox WHERE dk = ?", dk).one().n;
    const all = this.sql.exec<{ n: number; b: number }>("SELECT COUNT(*) AS n, COALESCE(SUM(size), 0) AS b FROM mailbox").one();
    if (mine >= MBOX_PER_DEVICE || all.n >= MBOX_PER_COMPUTER || all.b + blob.length > MBOX_MAX_BYTES) return err("full");
    this.sql.exec(
      "INSERT INTO mailbox(dk, nonce, blob, size, exp, state) VALUES (?, ?, ?, ?, ?, 'queued')",
      dk,
      nonce,
      blob.slice().buffer,
      blob.length,
      exp,
    );
    send(ws, JSON.stringify({ t: "mbox.ok", nonce, state: "queued" }));
    return true;
  }

  cancel(ws: WebSocket, dk: string, msg: Msg): void {
    const nonce = typeof msg.nonce === "string" ? msg.nonce : "";
    const row = this.sql.exec<{ state: string }>("SELECT state FROM mailbox WHERE dk = ? AND nonce = ?", dk, nonce).toArray()[0];
    if (row?.state === "queued") {
      this.sql.exec("DELETE FROM mailbox WHERE dk = ? AND nonce = ?", dk, nonce);
      return send(ws, JSON.stringify({ t: "mbox.cancelled", nonce }));
    }
    send(ws, JSON.stringify({ t: "mbox.gone", nonce, state: row ? "delivering" : "unknown" }));
  }

  list(ws: WebSocket, dk: string): void {
    const items = this.sql
      .exec<{ nonce: string; exp: number; state: string }>("SELECT nonce, exp, state FROM mailbox WHERE dk = ? ORDER BY seq", dk)
      .toArray();
    send(ws, JSON.stringify({ t: "mbox.items", items }));
  }

  /** The host's `mbox.ack`: the item is done either way, and its sender hears how it went. */
  ack(msg: Msg, toDevice: ToDevice): void {
    const row = this.sql
      .exec<{ dk: string; nonce: string }>("SELECT dk, nonce FROM mailbox WHERE seq = ? AND state = 'delivering'", Number(msg.seq))
      .toArray()[0];
    if (!row) return;
    this.sql.exec("DELETE FROM mailbox WHERE seq = ?", Number(msg.seq));
    const note =
      msg.ok === true
        ? { t: "mbox.delivered", nonce: row.nonce }
        : { t: "mbox.failed", nonce: row.nonce, code: FAIL_CODES.has(msg.code as string) ? msg.code : "invalid" };
    toDevice(row.dk, note);
  }

  /** One item in flight at a time, in `seq` order; the next goes out after the host's ack. */
  deliver(host: WebSocket, now: number): void {
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
    send(
      host,
      JSON.stringify({ t: "mbox.item", seq: row.seq, from: row.dk, nonce: row.nonce, d: b64url(new Uint8Array(row.blob)), exp: row.exp }),
    );
  }

  /** A blocked device's queued items never reach the host. */
  drop(dk: string): void {
    this.sql.exec("DELETE FROM mailbox WHERE dk = ? AND state = 'queued'", dk);
  }

  expire(now: number, toDevice: ToDevice): void {
    for (const row of this.sql.exec<{ dk: string; nonce: string }>("SELECT dk, nonce FROM mailbox WHERE exp <= ?", now).toArray()) {
      toDevice(row.dk, { t: "mbox.expired", nonce: row.nonce });
    }
    this.sql.exec("DELETE FROM mailbox WHERE exp <= ?", now);
  }

  /** The host went away: what it didn't ack goes back to the queue. */
  requeue(): void {
    this.sql.exec("UPDATE mailbox SET state = 'queued' WHERE state = 'delivering'");
  }

  any(): boolean {
    return this.sql.exec("SELECT 1 FROM mailbox LIMIT 1").toArray().length > 0;
  }
}
