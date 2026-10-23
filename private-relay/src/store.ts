/** SQLite for computers, ACL, signature nonces and the offline mailbox. Sockets stay in memory. */

import { chmodSync, mkdirSync } from 'node:fs'
import { DatabaseSync } from 'node:sqlite'
import { NONCE_TTL_MS } from './sig.ts'

export interface ComputerRow {
  id: string
  signPub: string
  status: string
}

export interface MailboxRow {
  seq: number
  dk: string
  nonce: string
  blob: Uint8Array
  exp: number
  state: string
}

export class RelayStore {
  readonly db: DatabaseSync

  constructor(file: string) {
    const dir = file.slice(0, file.lastIndexOf('/')) || '.'
    mkdirSync(dir, { recursive: true, mode: 0o700 })
    try {
      chmodSync(dir, 0o700)
    } catch {
      // Windows has no Unix mode bits.
    }
    this.db = new DatabaseSync(file)
    this.db.exec(`
      PRAGMA journal_mode = WAL;
      PRAGMA busy_timeout = 5000;
      CREATE TABLE IF NOT EXISTS computers(
        id TEXT PRIMARY KEY,
        sign_pub TEXT NOT NULL,
        box_pub TEXT NOT NULL,
        name TEXT NOT NULL,
        platform TEXT NOT NULL,
        device TEXT,
        version TEXT NOT NULL,
        status TEXT NOT NULL DEFAULT 'active',
        created_at INTEGER NOT NULL,
        updated_at INTEGER NOT NULL
      );
      CREATE TABLE IF NOT EXISTS acl(
        computer_id TEXT PRIMARY KEY,
        ver INTEGER NOT NULL,
        body TEXT NOT NULL
      );
      CREATE TABLE IF NOT EXISTS nonces(
        kid TEXT NOT NULL,
        nonce TEXT NOT NULL,
        exp INTEGER NOT NULL,
        PRIMARY KEY(kid, nonce)
      );
      CREATE TABLE IF NOT EXISTS mailbox(
        seq INTEGER PRIMARY KEY AUTOINCREMENT,
        computer_id TEXT NOT NULL,
        dk TEXT NOT NULL,
        nonce TEXT NOT NULL,
        blob BLOB NOT NULL,
        size INTEGER NOT NULL,
        exp INTEGER NOT NULL,
        state TEXT NOT NULL,
        UNIQUE(computer_id, dk, nonce)
      );
    `)
    // A crash leaves the in-flight row marked delivering, and the new process has no host to ack it.
    this.db.exec("UPDATE mailbox SET state = 'queued' WHERE state = 'delivering'")
    try {
      chmodSync(file, 0o600)
    } catch {
      // Windows has no Unix mode bits. The directory is still the operator's.
    }
  }

  close(): void {
    this.db.close()
  }

  computer(id: string): ComputerRow | null {
    const row = this.db.prepare('SELECT id, sign_pub, status FROM computers WHERE id = ?').get(id) as
      | { id: string; sign_pub: string; status: string }
      | undefined
    return row ? { id: row.id, signPub: row.sign_pub, status: row.status } : null
  }

  countComputers(): number {
    const row = this.db.prepare('SELECT COUNT(*) AS n FROM computers').get() as { n: number }
    return row.n
  }

  /** Insert or refresh a computer. The id is derived from `signPub`, so the key does not change. */
  register(row: {
    id: string
    signPub: string
    boxPub: string
    name: string
    platform: string
    device: string | null
    version: string
    now: number
  }): void {
    this.db
      .prepare(
        `INSERT INTO computers(id, sign_pub, box_pub, name, platform, device, version, status, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'active', ?, ?)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, device = excluded.device, version = excluded.version,
           box_pub = excluded.box_pub, updated_at = excluded.updated_at`,
      )
      .run(row.id, row.signPub, row.boxPub, row.name, row.platform, row.device, row.version, row.now, row.now)
  }

  /** `false` when this nonce was already seen and is still inside its 10-minute window. */
  rememberNonce(kid: string, nonce: string, now: number): boolean {
    this.db.prepare('DELETE FROM nonces WHERE exp <= ?').run(now)
    const result = this.db.prepare('INSERT OR IGNORE INTO nonces(kid, nonce, exp) VALUES (?, ?, ?)').run(kid, nonce, now + NONCE_TTL_MS)
    return result.changes === 1
  }

  acl(computerId: string): { ver: number; body: string } | null {
    const row = this.db.prepare('SELECT ver, body FROM acl WHERE computer_id = ?').get(computerId) as
      | { ver: number; body: string }
      | undefined
    return row ?? null
  }

  saveAcl(computerId: string, ver: number, body: string): void {
    this.db.prepare('INSERT INTO acl(computer_id, ver, body) VALUES (?, ?, ?) ON CONFLICT(computer_id) DO UPDATE SET ver = excluded.ver, body = excluded.body').run(computerId, ver, body)
  }

  mailboxByNonce(computerId: string, dk: string, nonce: string): { state: string } | null {
    const row = this.db.prepare('SELECT state FROM mailbox WHERE computer_id = ? AND dk = ? AND nonce = ?').get(computerId, dk, nonce) as
      | { state: string }
      | undefined
    return row ?? null
  }

  mailboxCounts(computerId: string, dk: string): { mine: number; all: number; bytes: number } {
    const mine = this.db.prepare('SELECT COUNT(*) AS n FROM mailbox WHERE computer_id = ? AND dk = ?').get(computerId, dk) as { n: number }
    const all = this.db.prepare('SELECT COUNT(*) AS n, COALESCE(SUM(size), 0) AS b FROM mailbox WHERE computer_id = ?').get(computerId) as { n: number; b: number }
    return { mine: mine.n, all: all.n, bytes: all.b }
  }

  insertMailbox(computerId: string, dk: string, nonce: string, blob: Uint8Array, exp: number): void {
    this.db
      .prepare("INSERT INTO mailbox(computer_id, dk, nonce, blob, size, exp, state) VALUES (?, ?, ?, ?, ?, ?, 'queued')")
      .run(computerId, dk, nonce, Buffer.from(blob), blob.length, exp)
  }

  cancelQueued(computerId: string, dk: string, nonce: string): boolean {
    const result = this.db
      .prepare("DELETE FROM mailbox WHERE computer_id = ? AND dk = ? AND nonce = ? AND state = 'queued'")
      .run(computerId, dk, nonce)
    return result.changes === 1
  }

  mailboxState(computerId: string, dk: string, nonce: string): string | null {
    return this.mailboxByNonce(computerId, dk, nonce)?.state ?? null
  }

  listMailbox(computerId: string, dk: string): { nonce: string; exp: number; state: string }[] {
    return this.db
      .prepare('SELECT nonce, exp, state FROM mailbox WHERE computer_id = ? AND dk = ? ORDER BY seq')
      .all(computerId, dk) as { nonce: string; exp: number; state: string }[]
  }

  delivering(computerId: string): boolean {
    const row = this.db.prepare("SELECT 1 AS n FROM mailbox WHERE computer_id = ? AND state = 'delivering'").get(computerId) as { n: number } | undefined
    return row != null
  }

  nextQueued(computerId: string, now: number): MailboxRow | null {
    const row = this.db
      .prepare(
        `SELECT seq, dk, nonce, blob, exp, state FROM mailbox
         WHERE computer_id = ? AND state = 'queued' AND exp > ? ORDER BY seq LIMIT 1`,
      )
      .get(computerId, now) as { seq: number; dk: string; nonce: string; blob: Uint8Array; exp: number; state: string } | undefined
    return row ?? null
  }

  markDelivering(seq: number): void {
    this.db.prepare("UPDATE mailbox SET state = 'delivering' WHERE seq = ?").run(seq)
  }

  /** The item the host is acknowledging, if it is the one in flight. */
  takeDelivering(computerId: string, seq: number): { dk: string; nonce: string } | null {
    const row = this.db
      .prepare("SELECT dk, nonce FROM mailbox WHERE seq = ? AND computer_id = ? AND state = 'delivering'")
      .get(seq, computerId) as { dk: string; nonce: string } | undefined
    if (!row) return null
    this.db.prepare('DELETE FROM mailbox WHERE seq = ?').run(seq)
    return row
  }

  requeueDelivering(computerId: string): void {
    this.db.prepare("UPDATE mailbox SET state = 'queued' WHERE computer_id = ? AND state = 'delivering'").run(computerId)
  }

  expired(computerId: string, now: number): { dk: string; nonce: string }[] {
    return this.db.prepare('SELECT dk, nonce FROM mailbox WHERE computer_id = ? AND exp <= ?').all(computerId, now) as { dk: string; nonce: string }[]
  }

  deleteExpired(computerId: string, now: number): void {
    this.db.prepare('DELETE FROM mailbox WHERE computer_id = ? AND exp <= ?').run(computerId, now)
  }
}
