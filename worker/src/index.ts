import { ApnsClient, Notification, PushType, Priority } from "@fivesheepco/cloudflare-apns2";

export interface Env {
  APNS_TEAM_ID: string;
  APNS_KEY_ID: string;
  APNS_SIGNING_KEY: string;
  API_SECRET: string;
}

interface PushRequest {
  pushToken: string;
  event?: "update" | "end";
  contentState?: Record<string, unknown>;
  // Alert push fields
  type?: "liveactivity" | "alert";
  title?: string;
  body?: string;
}

// Cache ApnsClient instances at module level — reuses JWT token across requests
// within the same Worker isolate (token valid 20-60 min per Apple docs)
let cachedAlertClient: ApnsClient | null = null;
let cachedLiveActivityClient: ApnsClient | null = null;
let cachedEnvHash: string | null = null;

function getClients(env: Env): { alertClient: ApnsClient; liveActivityClient: ApnsClient } {
  const envHash = `${env.APNS_TEAM_ID}:${env.APNS_KEY_ID}`;
  if (cachedAlertClient && cachedLiveActivityClient && cachedEnvHash === envHash) {
    return { alertClient: cachedAlertClient, liveActivityClient: cachedLiveActivityClient };
  }

  const signingKey = env.APNS_SIGNING_KEY.replace(/\\n/g, "\n");
  const bundleId = "com.pokai.Codync.ios";

  cachedAlertClient = new ApnsClient({
    team: env.APNS_TEAM_ID,
    keyId: env.APNS_KEY_ID,
    signingKey,
    defaultTopic: bundleId,
    host: "api.sandbox.push.apple.com",
  });

  cachedLiveActivityClient = new ApnsClient({
    team: env.APNS_TEAM_ID,
    keyId: env.APNS_KEY_ID,
    signingKey,
    defaultTopic: `${bundleId}.push-type.liveactivity`,
    host: "api.sandbox.push.apple.com",
  });

  cachedEnvHash = envHash;
  return { alertClient: cachedAlertClient, liveActivityClient: cachedLiveActivityClient };
}

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    if (request.method !== "POST") {
      return new Response("Method not allowed", { status: 405 });
    }

    const auth = request.headers.get("Authorization");
    if (auth !== `Bearer ${env.API_SECRET}`) {
      return new Response("Unauthorized", { status: 401 });
    }

    let body: PushRequest;
    try {
      body = await request.json() as PushRequest;
    } catch {
      return new Response("Invalid JSON", { status: 400 });
    }

    if (!body.pushToken) {
      return new Response("Missing pushToken", { status: 400 });
    }

    const { alertClient, liveActivityClient } = getClients(env);
    const pushType = body.type ?? "liveactivity";

    if (pushType === "alert") {
      // Regular alert notification (e.g., session completed)
      const notification = new Notification(body.pushToken, {
        type: PushType.alert,
        priority: Priority.immediate,
        aps: {
          alert: {
            title: body.title ?? "Codync",
            body: body.body ?? "",
          },
          sound: "default",
        },
      });

      try {
        await alertClient.send(notification);
        return Response.json({ success: true });
      } catch (err: unknown) {
        const message = err instanceof Error ? err.message : String(err);
        console.error("APNs alert error:", message);
        return Response.json({ success: false, error: message }, { status: 502 });
      }
    }

    // Live Activity push
    const event = body.event ?? "update";

    const aps: Record<string, unknown> = {
      timestamp: Math.floor(Date.now() / 1000),
      event,
    };

    if (event === "update" && body.contentState) {
      aps["content-state"] = body.contentState;
    }

    if (event === "end") {
      aps["dismissal-date"] = Math.floor(Date.now() / 1000);
    }

    const notification = new Notification(body.pushToken, {
      type: PushType.liveactivity,
      priority: Priority.throttled,
      aps,
    });

    try {
      await liveActivityClient.send(notification);
      return Response.json({ success: true });
    } catch (err: unknown) {
      const message = err instanceof Error ? err.message : String(err);
      console.error("APNs error:", message);
      return Response.json({ success: false, error: message }, { status: 502 });
    }
  },
};
