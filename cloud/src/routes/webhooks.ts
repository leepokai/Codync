// Clerk webhooks: a deleted user takes their account, devices, grants and computers with them.

import { Webhook } from "svix";
import type { Env } from "../index";
import { ApiError, audit, notify, revokeGrants, type Ctx } from "./common";
import { deleteComputerIcons } from "./icons";

export async function clerkWebhook(c: Ctx) {
  if (!c.env.CLERK_WEBHOOK_SECRET) throw new ApiError("internal", "Webhook secret not configured");
  const payload = new TextDecoder().decode(c.raw);
  const h = (name: string) => c.req.headers.get(name) ?? "";
  let event: { type?: string; data?: { id?: unknown } } | null;
  try {
    // Throws unless the signature and timestamp check out; returns nothing.
    new Webhook(c.env.CLERK_WEBHOOK_SECRET).verify(payload, {
      "svix-id": h("svix-id"),
      "svix-timestamp": h("svix-timestamp"),
      "svix-signature": h("svix-signature"),
    });
    event = JSON.parse(payload);
  } catch {
    throw new ApiError("badSignature");
  }
  const seen = await c.env.DB.prepare("SELECT 1 FROM processed_events WHERE provider = 'clerk' AND event_id = ?")
    .bind(h("svix-id"))
    .first();
  if (seen) return {};
  if (event?.type === "user.deleted" && typeof event.data?.id === "string") await deleteAccount(c.env, event.data.id, c.now);
  await c.env.DB.prepare("INSERT OR IGNORE INTO processed_events(provider, event_id, processed_at) VALUES ('clerk', ?, ?)")
    .bind(h("svix-id"), c.now)
    .run();
  return {};
}

async function deleteAccount(env: Env, userId: string, now: number): Promise<void> {
  const { stmts, blocks } = await revokeGrants(
    env,
    "d.owner_user_id = ?1 OR g.computer_id IN (SELECT id FROM computers WHERE owner_user_id = ?1)",
    [userId],
    "accountDeleted",
    now,
  );
  const owned = await env.DB.prepare("SELECT id FROM computers WHERE owner_user_id = ?").bind(userId).all<{ id: string }>();
  // Revoke access first. R2 availability must never keep a deleted account's sockets alive.
  // Retain computer ownership until storage cleanup succeeds so retries can find every slot.
  await env.DB.batch([
    ...stmts,
    env.DB.prepare(
      `INSERT INTO accounts(user_id, status, created_at, deleted_at) VALUES (?1, 'deleted', ?2, ?2)
       ON CONFLICT(user_id) DO UPDATE SET status = 'deleted', deleted_at = ?2`,
    ).bind(userId, now),
    env.DB.prepare("UPDATE devices SET revoked_at = ? WHERE owner_user_id = ? AND revoked_at IS NULL").bind(now, userId),
    env.DB.prepare(
      `UPDATE access_requests SET status = 'cancelled', decided_at = ?1
       WHERE status = 'pending' AND (user_id = ?2 OR computer_id IN (SELECT id FROM computers WHERE owner_user_id = ?2))`,
    ).bind(now, userId),
    audit(env, "system", "account.delete", userId, "ok", now),
  ]);
  const computers = new Set([...blocks.keys(), ...owned.results.map((r) => r.id)]);
  await Promise.all([...computers].map((id) => notify(env, id, blocks.get(id))));
  for (const computer of owned.results) await deleteComputerIcons(env, computer.id);
  await env.DB.prepare("UPDATE computers SET owner_user_id = NULL, claimed_at = NULL, updated_at = ? WHERE owner_user_id = ?")
    .bind(now, userId).run();
  await Promise.all(owned.results.map(({ id }) => notify(env, id)));
}
