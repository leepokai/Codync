// Short pairing codes (docs/plans/require-sign-in.md, phase 3): a computer registers its pairing link under a code
// people can type where scanning a terminal is awkward; a signed-in app trades the code for the link once.
// The computer's own check (the 6-digit code it shows when it asks) catches a cloud that hands out another link.

import { ApiError, body, host, user, type Ctx } from "./common";

const TTL_MS = 10 * 60_000;
const LIVE_PER_COMPUTER = 5;
const LETTERS = "ABCDEFGHJKLMNPQRSTUVWXYZ";

/** `QRTX-4821`: four letters (no I or O) and four digits. */
function newCode(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(8));
  const letters = Array.from(bytes.subarray(0, 4), (b) => LETTERS[b % LETTERS.length]).join("");
  const digits = Array.from(bytes.subarray(4), (b) => String(b % 10)).join("");
  return `${letters}-${digits}`;
}

/** What people type, as stored: case, spaces and the dash don't matter. */
export function normalizeCode(raw: string): string | null {
  const s = raw.toUpperCase().replace(/[\s-]/g, "");
  return /^[A-Z]{4}[0-9]{4}$/.test(s) ? `${s.slice(0, 4)}-${s.slice(4)}` : null;
}

export async function createPairingCode(c: Ctx) {
  const comp = await host(c);
  const b = body(c);
  const link = b.link;
  if (typeof link !== "string" || link.length > 4096 || !/^codync(-dev)?:\/\/pair\?/.test(link)) {
    throw new ApiError("badRequest", "Invalid link");
  }
  const expiresAt = Math.min(typeof b.expiresAt === "number" ? b.expiresAt : c.now + TTL_MS, c.now + TTL_MS);
  await c.env.DB.prepare("DELETE FROM pairing_codes WHERE computer_id = ? AND (expires_at <= ? OR used_at IS NOT NULL)")
    .bind(comp.id, c.now)
    .run();
  const live = await c.env.DB.prepare("SELECT count(*) AS n FROM pairing_codes WHERE computer_id = ?").bind(comp.id).first<{ n: number }>();
  if ((live?.n ?? 0) >= LIVE_PER_COMPUTER) {
    await c.env.DB.prepare(
      "DELETE FROM pairing_codes WHERE code = (SELECT code FROM pairing_codes WHERE computer_id = ? ORDER BY expires_at LIMIT 1)",
    )
      .bind(comp.id)
      .run();
  }
  for (let attempt = 0; attempt < 3; attempt++) {
    const code = newCode();
    const r = await c.env.DB.prepare(
      "INSERT INTO pairing_codes(code, computer_id, link, expires_at) VALUES (?, ?, ?, ?) ON CONFLICT(code) DO NOTHING",
    )
      .bind(code, comp.id, link, expiresAt)
      .run();
    if (r.meta.changes === 1) return { code, expiresAt };
  }
  throw new ApiError("conflict", "Try again");
}

export async function redeemPairingCode(c: Ctx, [raw]: string[]) {
  await user(c);
  const code = normalizeCode(decodeURIComponent(raw ?? ""));
  if (!code) throw new ApiError("notFound", "That code isn't valid. Check it on the computer.");
  const row = await c.env.DB.prepare(
    "UPDATE pairing_codes SET used_at = ?1 WHERE code = ?2 AND used_at IS NULL AND expires_at > ?1 RETURNING link",
  )
    .bind(c.now, code)
    .first<{ link: string }>();
  if (!row) throw new ApiError("notFound", "That code expired or was used. Show a new one on the computer.");
  return { link: row.link };
}
