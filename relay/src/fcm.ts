/** FCM HTTP v1. Credentials stay in the Worker; hosts receive opaque tickets only. */
export interface ServiceAccount {
  project_id: string;
  client_email: string;
  private_key: string;
}

export class FcmError extends Error {
  readonly reason: string;
  readonly gone: boolean;
  constructor(reason: string, gone = false) { super(reason); this.reason = reason; this.gone = gone; }
}

const encoder = new TextEncoder();
const base64url = (data: Uint8Array) => btoa(String.fromCharCode(...data)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
const encoded = (value: unknown) => base64url(encoder.encode(JSON.stringify(value)));
const OAUTH_URL = "https://oauth2.googleapis.com/token";

export class FcmClient {
  private token?: { value: string; expiresAt: number };
  private refreshing?: Promise<string>;
  private key?: CryptoKey;
  private readonly account: ServiceAccount;
  private readonly send: typeof fetch;

  constructor(account: ServiceAccount, send: typeof fetch = fetch) {
    this.account = account;
    // Workers' native fetch requires its global receiver, even when injected.
    this.send = send.bind(globalThis);
    if (!account || typeof account.project_id !== "string" || typeof account.client_email !== "string" ||
        typeof account.private_key !== "string" || !/^[a-z][a-z0-9-]{4,61}[a-z0-9]$/.test(account.project_id) ||
        !account.client_email.endsWith(".iam.gserviceaccount.com") || !account.private_key.includes("BEGIN PRIVATE KEY")) {
      throw new FcmError("invalid FCM service account");
    }
  }

  private async accessToken(): Promise<string> {
    if (this.token && this.token.expiresAt > Date.now() + 60_000) return this.token.value;
    if (this.refreshing) return this.refreshing;
    this.refreshing = this.refresh();
    try { return await this.refreshing; }
    finally { this.refreshing = undefined; }
  }

  private async refresh(): Promise<string> {
    if (!this.key) {
      const pem = this.account.private_key.replace(/\\n/g, "\n").replace(/-----[^-]+-----/g, "").replace(/\s/g, "");
      const bytes = Uint8Array.from(atob(pem), char => char.charCodeAt(0));
      this.key = await crypto.subtle.importKey("pkcs8", bytes,
        { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, false, ["sign"]);
    }
    const now = Math.floor(Date.now() / 1000);
    const input = encoded({ alg: "RS256", typ: "JWT" }) + "." + encoded({
      iss: this.account.client_email, scope: "https://www.googleapis.com/auth/firebase.messaging",
      aud: OAUTH_URL, iat: now, exp: now + 3600,
    });
    const signature = await crypto.subtle.sign("RSASSA-PKCS1-v1_5", this.key, encoder.encode(input));
    const response = await this.send(OAUTH_URL, {
      method: "POST", headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({ grant_type: "urn:ietf:params:oauth:grant-type:jwt-bearer", assertion: input + "." + base64url(new Uint8Array(signature)) }),
      signal: AbortSignal.timeout(15_000),
    });
    if (!response.ok) throw new FcmError("FCM authorization failed");
    const body = await response.json() as { access_token?: string; expires_in?: number };
    if (!body.access_token || !Number.isFinite(body.expires_in) || body.expires_in! <= 60) throw new FcmError("invalid FCM authorization response");
    this.token = { value: body.access_token, expiresAt: Date.now() + Math.min(body.expires_in!, 3600) * 1000 };
    return body.access_token;
  }

  async push(token: string, data: Record<string, string>, urgent: boolean, validateOnly = false): Promise<void> {
    const message = { fid: token, data, android: { priority: urgent ? "HIGH" : "NORMAL", ttl: "3600s" } };
    if (encoder.encode(JSON.stringify(message)).length > 4096) throw new FcmError("payload too large");
    for (let attempt = 0; attempt < 2; attempt++) {
      const accessToken = await this.accessToken();
      const response = await this.send(`https://fcm.googleapis.com/v1/projects/${this.account.project_id}/messages:send`, {
        method: "POST", headers: { Authorization: `Bearer ${accessToken}`, "Content-Type": "application/json" },
        body: JSON.stringify({ message, ...(validateOnly ? { validate_only: true } : {}) }), signal: AbortSignal.timeout(15_000),
      });
      if (response.ok) return;
      if (response.status === 401 && attempt === 0) { this.token = undefined; continue; }
      const failure = await response.json().catch(() => ({})) as {
        error?: { status?: string; details?: Array<{ "@type"?: string; errorCode?: string }> };
      };
      const detail = failure.error?.details?.find(item => item["@type"] === "type.googleapis.com/google.firebase.fcm.v1.FcmError");
      // Invalid payloads, wrong project/sender and invalid credentials do not revoke a phone.
      const gone = response.status === 404 && detail?.errorCode === "UNREGISTERED";
      const reason = detail?.errorCode ?? failure.error?.status ?? `FCM HTTP ${response.status}`;
      throw new FcmError(reason, gone);
    }
  }
}

let configured: { raw: string; client: FcmClient } | undefined;
export function fcmClient(raw: string | undefined): FcmClient {
  if (!raw) throw new FcmError("FCM is not configured");
  if (configured?.raw === raw) return configured.client;
  let account: ServiceAccount;
  try { account = JSON.parse(raw) as ServiceAccount; }
  catch { throw new FcmError("invalid FCM service account"); }
  const client = new FcmClient(account);
  configured = { raw, client };
  return client;
}
