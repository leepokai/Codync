// Private, host-encrypted icon storage. Fixed slots bound storage to 8 MiB per computer;
// neither filenames, image URLs nor encryption keys are sent to the cloud.
import { ApiError, host, type Ctx } from "./common";

export const MAX_ICON_BYTES = 64 * 1024;
const ICON_SLOTS = 128;

export async function deleteComputerIcons(env: Ctx["env"], computerId: string): Promise<void> {
  if (env.ICONS) await env.ICONS.delete(Array.from({ length: ICON_SLOTS }, (_, slot) => `${computerId}/${slot}`));
}

export async function icon(c: Ctx, [slot]: string[]): Promise<Response> {
  if (!/^(0|[1-9]\d{0,2})$/.test(slot) || Number(slot) >= ICON_SLOTS) throw new ApiError("badRequest");
  const computer = await host(c);
  if (!computer.owner_user_id) throw new ApiError("forbidden", "Sign in and claim this computer to store custom icons");
  const account = await c.env.DB.prepare("SELECT status FROM accounts WHERE user_id = ?")
    .bind(computer.owner_user_id).first<{ status: string }>();
  if (account?.status !== "active") throw new ApiError("forbidden");
  if (!c.env.ICONS) throw new ApiError("unavailable", "Icon storage is not configured", 503);
  if (c.req.method !== "GET" && c.env.ICON_LIMITER &&
      !(await c.env.ICON_LIMITER.limit({ key: computer.owner_user_id })).success) throw new ApiError("rateLimited");
  const key = `${computer.id}/${slot}`;
  const headers = { "Cache-Control": "no-store", "X-Content-Type-Options": "nosniff" };
  if (c.req.method === "PUT") {
    if (c.raw.length > MAX_ICON_BYTES) throw new ApiError("tooLarge");
    // Version byte + nonce + AEAD tag. Only the host can validate the encrypted image.
    if (c.raw.length < 30 || c.raw[0] !== 1) throw new ApiError("badRequest", "Invalid encrypted icon");
    await c.env.ICONS.put(key, c.raw, { httpMetadata: { contentType: "application/octet-stream" } });
    // Account deletion may race the upload after authentication; don't leave that image behind.
    const stillOwned = await c.env.DB.prepare(
      "SELECT 1 FROM computers c JOIN accounts a ON a.user_id = c.owner_user_id WHERE c.id = ? AND c.owner_user_id = ? AND c.status = 'active' AND a.status = 'active'",
    ).bind(computer.id, computer.owner_user_id).first();
    if (!stillOwned) {
      await c.env.ICONS.delete(key);
      throw new ApiError("forbidden");
    }
    return new Response(null, { status: 204, headers });
  }
  if (c.req.method === "DELETE") {
    await c.env.ICONS.delete(key);
    return new Response(null, { status: 204, headers });
  }
  const object = await c.env.ICONS.get(key);
  if (!object) throw new ApiError("notFound");
  return new Response(object.body, { headers: { ...headers, "Content-Type": "application/octet-stream" } });
}
