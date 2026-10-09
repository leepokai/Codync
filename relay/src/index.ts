// Codync push relay.
//
// The APNs key lives only here. The phone registers its device (or Live
// Activity) token and gets back an opaque, encrypted *ticket*; it hands the
// ticket to its own codync-host, which can then ask the relay to push to that
// one device — without ever learning the raw token or holding a shared secret.
//
//   POST /register { token, env: "sandbox" | "production", kind: "alert" | "liveactivity" } -> { ticket }
//   POST /push     { ticket, alert?: { title, body }, threadId?, category?, data?, mutableContent?,
//                    liveActivity?: { event: "update" | "end", contentState } }
//
// `mutableContent: true` sets `mutable-content: 1` so the Notification Service Extension can replace the
// generic alert with the host's end-to-end encrypted title/body (`data.sealed`).

import { ApnsClient, Notification, PushType, Priority } from "@fivesheepco/cloudflare-apns2";

export interface Env {
  APNS_TEAM_ID: string;
  APNS_KEY_ID: string;
  APNS_SIGNING_KEY: string;
  /** base64 of 32 random bytes: `openssl rand -base64 32` */
  TICKET_KEY: string;
}

type ApnsEnv = "sandbox" | "production";
type Kind = "alert" | "liveactivity";
interface TicketPayload { t: string; e: ApnsEnv; k: Kind; b?: string }

const BUNDLE_ID = "com.pokai.Codync.ios";

// ---- tickets (AES-GCM, key from TICKET_KEY) ----

const b64url = (bytes: Uint8Array) =>
  btoa(String.fromCharCode(...bytes)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");

const fromB64url = (s: string) => {
  const b64 = s.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((s.length + 3) % 4);
  return Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
};

let cachedKey: { raw: string; key: CryptoKey } | null = null;

async function ticketKey(env: Env): Promise<CryptoKey> {
  if (cachedKey?.raw === env.TICKET_KEY) return cachedKey.key;
  const raw = Uint8Array.from(atob(env.TICKET_KEY), (c) => c.charCodeAt(0));
  const key = await crypto.subtle.importKey("raw", raw, "AES-GCM", false, ["encrypt", "decrypt"]);
  cachedKey = { raw: env.TICKET_KEY, key };
  return key;
}

export async function sealTicket(env: Env, p: TicketPayload): Promise<string> {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = new Uint8Array(
    await crypto.subtle.encrypt({ name: "AES-GCM", iv }, await ticketKey(env), new TextEncoder().encode(JSON.stringify(p))),
  );
  const out = new Uint8Array(iv.length + ct.length);
  out.set(iv);
  out.set(ct, iv.length);
  return b64url(out);
}

export async function openTicket(env: Env, ticket: string): Promise<TicketPayload | null> {
  try {
    const bytes = fromB64url(ticket);
    const pt = await crypto.subtle.decrypt({ name: "AES-GCM", iv: bytes.slice(0, 12) }, await ticketKey(env), bytes.slice(12));
    return JSON.parse(new TextDecoder().decode(pt)) as TicketPayload;
  } catch {
    return null;
  }
}

// ---- APNs clients, cached per isolate (JWT reused while valid) ----

const clients = new Map<string, ApnsClient>();

export function apnsTopic(bundleId: string | undefined, kind: Kind): string {
  const bundle = bundleId ?? BUNDLE_ID;
  if (bundle !== BUNDLE_ID && bundle !== `${BUNDLE_ID}.dev`) throw new Error("invalid bundle identifier");
  return kind === "liveactivity" ? `${bundle}.push-type.liveactivity` : bundle;
}

function client(env: Env, apnsEnv: ApnsEnv, kind: Kind, bundleId?: string): ApnsClient {
  const topic = apnsTopic(bundleId, kind);
  const id = `${env.APNS_KEY_ID}:${apnsEnv}:${topic}`;
  let c = clients.get(id);
  if (!c) {
    c = new ApnsClient({
      team: env.APNS_TEAM_ID,
      keyId: env.APNS_KEY_ID,
      signingKey: env.APNS_SIGNING_KEY.replace(/\\n/g, "\n"),
      defaultTopic: topic,
      host: apnsEnv === "production" ? "api.push.apple.com" : "api.sandbox.push.apple.com",
    });
    clients.set(id, c);
  }
  return c;
}

const json = (body: unknown, status = 200) => Response.json(body, { status });

async function register(req: Request, env: Env): Promise<Response> {
  const body = (await req.json().catch(() => null)) as { token?: string; env?: string; kind?: string; bundleId?: string } | null;
  if (typeof body?.token !== "string" || !/^(?:[0-9a-fA-F]{2}){16,100}$/.test(body.token)) return json({ error: "invalid token" }, 400);
  if (body.env !== "production" && body.env !== "sandbox") return json({ error: "invalid environment" }, 400);
  if (body.kind !== "alert" && body.kind !== "liveactivity") return json({ error: "invalid kind" }, 400);
  if (body.bundleId !== undefined && body.bundleId !== BUNDLE_ID && body.bundleId !== `${BUNDLE_ID}.dev`) return json({ error: "invalid bundle identifier" }, 400);
  const e: ApnsEnv = body.env;
  const k: Kind = body.kind === "liveactivity" ? "liveactivity" : "alert";
  return json({ ticket: await sealTicket(env, { t: body.token, e, k, ...(body.bundleId ? { b: body.bundleId } : {}) }) });
}

export interface PushBody {
  ticket?: string;
  alert?: { title?: string; body?: string };
  threadId?: string;
  category?: string;
  data?: Record<string, unknown>;
  mutableContent?: boolean;
  liveActivity?: { event?: "update" | "end"; timestamp?: number; staleDate?: number; contentState?: Record<string, unknown> };
}

/** The `aps` dictionary for an alert push. */
export function alertAps(body: PushBody & { alert: NonNullable<PushBody["alert"]> }): Record<string, unknown> {
  const aps: Record<string, unknown> = {
    alert: { title: (body.alert.title ?? "Codync").slice(0, 120), body: (body.alert.body ?? "").slice(0, 400) },
    sound: "default",
    "thread-id": body.threadId,
    category: body.category,
  };
  if (body.mutableContent === true) aps["mutable-content"] = 1;
  return aps;
}

/** ActivityKit dates use Unix seconds; startedAt inside content-state uses Swift's Date epoch. */
export function liveActivityAps(la: NonNullable<PushBody["liveActivity"]>, now = Math.floor(Date.now() / 1000)): Record<string, unknown> {
  const state = la.contentState;
  if (!state || typeof state.status !== "string" || !["working", "needsInput", "idle", "error"].includes(state.status) ||
      state.activity !== "" || !(state.startedAt == null || (typeof state.startedAt === "number" && Number.isFinite(state.startedAt)))) {
    throw new Error("invalid contentState");
  }
  if (la.event !== "update" && la.event !== "end") throw new Error("invalid activity event");
  const timestamp = la.timestamp ?? now;
  const staleDate = la.staleDate ?? timestamp + 900;
  if (!Number.isSafeInteger(timestamp) || timestamp < 0 || timestamp > now + 60 ||
      !Number.isSafeInteger(staleDate) || staleDate < timestamp) throw new Error("invalid activity dates");
  const aps: Record<string, unknown> = {
    timestamp, event: la.event,
    "content-state": { status: state.status, activity: "", startedAt: state.startedAt ?? null },
  };
  if (la.event === "end") aps["dismissal-date"] = timestamp + 60;
  else aps["stale-date"] = staleDate;
  return aps;
}

export function tokenIsGone(error: { reason?: string; statusCode?: number }): boolean {
  return error.statusCode === 410 || ["Unregistered", "BadDeviceToken"].includes(error.reason ?? "");
}

async function push(req: Request, env: Env): Promise<Response> {
  const body = (await req.json().catch(() => null)) as PushBody | null;
  const t = body?.ticket ? await openTicket(env, body.ticket) : null;
  if (!body || !t) return json({ error: "invalid ticket" }, 403);
  // ponytail: no per-ticket rate limit; add a Durable Object counter if tickets get abused.
  try {
    if (t.k === "alert") {
      if (!body.alert || typeof body.alert !== "object" ||
          (body.alert.title !== undefined && typeof body.alert.title !== "string") ||
          (body.alert.body !== undefined && typeof body.alert.body !== "string") ||
          (body.data !== undefined && (!body.data || typeof body.data !== "object" || Array.isArray(body.data) || "aps" in body.data))) {
        return json({ error: "invalid alert" }, 400);
      }
      const notification = new Notification(t.t, {
          type: PushType.alert,
          priority: Priority.immediate,
          aps: alertAps({ ...body, alert: body.alert }),
          data: body.data ?? {},
          expiration: Math.floor(Date.now() / 1000) + 3600,
        });
      if (new TextEncoder().encode(JSON.stringify(notification.buildApnsOptions())).length > 4096) {
        return json({ error: "payload too large" }, 413);
      }
      await client(env, t.e, "alert", t.b).send(notification);
    } else {
      const la = body.liveActivity;
      if (!la) return json({ error: "liveActivity required" }, 400);
      let aps: Record<string, unknown>;
      try { aps = liveActivityAps(la); }
      catch { return json({ error: "invalid liveActivity" }, 400); }
      await client(env, t.e, "liveactivity", t.b).send(
        new Notification(t.t, {
          type: PushType.liveactivity,
          priority: la.event === "end" || la.contentState?.status === "needsInput" ? Priority.immediate : Priority.throttled,
          expiration: Number(aps["stale-date"] ?? aps["dismissal-date"]), aps,
        }),
      );
    }
    return json({ ok: true });
  } catch (err) {
    // ApnsError carries APNs' `reason` (e.g. "Unregistered") and `statusCode`.
    const e = err as { reason?: string; statusCode?: number; message?: string };
    const reason = e.reason ?? e.message ?? String(err);
    // Dead device token: tell the host to drop the ticket.
    const gone = tokenIsGone(e);
    return json({ error: reason, gone }, gone ? 410 : 502);
  }
}

/** One physical APNs destination receives only the newest registration for this event.
 * The host cannot do this comparison itself because it never sees raw APNs tokens.
 */
export function latestTicketIndices(tickets: Array<TicketPayload | null>): Set<number> {
  const latest = new Map<string, number>();
  tickets.forEach((ticket, index) => {
    if (ticket) latest.set(`${ticket.b ?? BUNDLE_ID}:${ticket.e}:${ticket.k}:${ticket.t.toLowerCase()}`, index);
  });
  return new Set(latest.values());
}

export async function pushBatch(req: Request, env: Env, deliver = push): Promise<Response> {
  const body = await req.json().catch(() => null) as { notifications?: PushBody[] } | null;
  const notifications = body?.notifications;
  if (!Array.isArray(notifications) || notifications.length === 0 || notifications.length > 256) {
    return json({ error: "invalid notification batch" }, 400);
  }
  const tickets = await Promise.all(notifications.map(notification =>
    typeof notification?.ticket === "string" ? openTicket(env, notification.ticket) : null));
  const latest = latestTicketIndices(tickets);
  const results = await Promise.all(notifications.map(async (notification, index) => {
    if (!tickets[index]) return { index, status: 403 };
    if (!latest.has(index)) return { index, status: 200, superseded: true };
    const response = await deliver(new Request(req.url, {
      method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(notification),
    }), env);
    return { index, status: response.status };
  }));
  return json({ results });
}

export default {
  async fetch(req: Request, env: Env): Promise<Response> {
    const { pathname } = new URL(req.url);
    if (req.method === "GET" && pathname === "/health") return json({ ok: true });
    if (req.method !== "POST") return json({ error: "method not allowed" }, 405);
    if (pathname === "/register") return register(req, env);
    if (pathname === "/push") return push(req, env);
    if (pathname === "/push-batch") return pushBatch(req, env);
    return json({ error: "not found" }, 404);
  },
};
