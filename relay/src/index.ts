// Codync push relay.
//
// The APNs key lives only here. The phone registers its device (or Live
// Activity) token and gets back an opaque, encrypted *ticket*; it hands the
// ticket to its own codync-host, which can then ask the relay to push to that
// one device — without ever learning the raw token or holding a shared secret.
//
//   POST /register { token, env: "sandbox" | "production", kind: "alert" | "liveactivity" } -> { ticket }
//   POST /push     { ticket, alert?: { title, body }, threadId?, category?, data?,
//                    liveActivity?: { event: "update" | "end", contentState } }

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
interface TicketPayload { t: string; e: ApnsEnv; k: Kind }

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

function client(env: Env, apnsEnv: ApnsEnv, kind: Kind): ApnsClient {
  const id = `${env.APNS_KEY_ID}:${apnsEnv}:${kind}`;
  let c = clients.get(id);
  if (!c) {
    c = new ApnsClient({
      team: env.APNS_TEAM_ID,
      keyId: env.APNS_KEY_ID,
      signingKey: env.APNS_SIGNING_KEY.replace(/\\n/g, "\n"),
      defaultTopic: kind === "liveactivity" ? `${BUNDLE_ID}.push-type.liveactivity` : BUNDLE_ID,
      host: apnsEnv === "production" ? "api.push.apple.com" : "api.sandbox.push.apple.com",
    });
    clients.set(id, c);
  }
  return c;
}

const json = (body: unknown, status = 200) => Response.json(body, { status });

async function register(req: Request, env: Env): Promise<Response> {
  const body = (await req.json().catch(() => null)) as { token?: string; env?: string; kind?: string } | null;
  if (!body?.token || !/^[0-9a-fA-F]{32,200}$/.test(body.token)) return json({ error: "invalid token" }, 400);
  const e: ApnsEnv = body.env === "production" ? "production" : "sandbox";
  const k: Kind = body.kind === "liveactivity" ? "liveactivity" : "alert";
  return json({ ticket: await sealTicket(env, { t: body.token, e, k }) });
}

interface PushBody {
  ticket?: string;
  alert?: { title?: string; body?: string };
  threadId?: string;
  category?: string;
  data?: Record<string, unknown>;
  liveActivity?: { event?: "update" | "end"; contentState?: Record<string, unknown> };
}

async function push(req: Request, env: Env): Promise<Response> {
  const body = (await req.json().catch(() => null)) as PushBody | null;
  const t = body?.ticket ? await openTicket(env, body.ticket) : null;
  if (!body || !t) return json({ error: "invalid ticket" }, 403);
  // ponytail: no per-ticket rate limit; add a Durable Object counter if tickets get abused.
  try {
    if (t.k === "alert") {
      if (!body.alert) return json({ error: "alert required" }, 400);
      await client(env, t.e, "alert").send(
        new Notification(t.t, {
          type: PushType.alert,
          priority: Priority.immediate,
          aps: {
            alert: { title: (body.alert.title ?? "Codync").slice(0, 120), body: (body.alert.body ?? "").slice(0, 400) },
            sound: "default",
            "thread-id": body.threadId,
            category: body.category,
          },
          data: body.data ?? {},
        }),
      );
    } else {
      const la = body.liveActivity;
      if (!la) return json({ error: "liveActivity required" }, 400);
      const now = Math.floor(Date.now() / 1000);
      const aps: Record<string, unknown> = { timestamp: now, event: la.event ?? "update", "content-state": la.contentState ?? {} };
      if (la.event === "end") aps["dismissal-date"] = now + 60;
      await client(env, t.e, "liveactivity").send(
        new Notification(t.t, { type: PushType.liveactivity, priority: Priority.immediate, aps }),
      );
    }
    return json({ ok: true });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    // 410 Unregistered / BadDeviceToken: tell the host to drop the ticket.
    const gone = /Unregistered|BadDeviceToken|ExpiredToken/.test(message);
    return json({ error: message, gone }, gone ? 410 : 502);
  }
}

export default {
  async fetch(req: Request, env: Env): Promise<Response> {
    const { pathname } = new URL(req.url);
    if (req.method === "GET" && pathname === "/health") return json({ ok: true });
    if (req.method !== "POST") return json({ error: "method not allowed" }, 405);
    if (pathname === "/register") return register(req, env);
    if (pathname === "/push") return push(req, env);
    return json({ error: "not found" }, 404);
  },
};
