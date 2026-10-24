// WeChat login is not OpenID Connect: it uses appid/secret, returns no id_token and
// usually no email. Clerk's custom social connection only speaks OIDC, so this bridge
// shows WeChat's QR page and signs an id_token whose subject is the WeChat unionid
// (openid when the open platform hasn't issued a unionid). The subject chosen for an
// openid is kept, and the access token lives for an hour after it is issued.

import type { Ctx } from "./routes/common";
import type { Env } from "./index";

const PREFIX = "/v1/wechat/oidc";
const TXN_TTL_MS = 10 * 60 * 1000;
const TOKEN_TTL_S = 60 * 60;
const DEFAULT_REDIRECTS = [
  "https://clerk.codync.dev/v1/oauth_callback",
  "https://sunny-mollusk-8651.clerk.accounts.dev/v1/oauth_callback",
];

interface Secrets {
  appId: string;
  appSecret: string;
  clientId: string;
  clientSecret: string;
  privateJwk: string;
  redirects: ReadonlySet<string>;
}

interface PrivateJwk extends JsonWebKey {
  kid?: string;
}

interface Signer {
  key: CryptoKey;
  publicJwk: JsonWebKey;
  kid: string;
}

interface Profile {
  subject: string;
  name: string | null;
  picture: string | null;
}

interface Txn {
  state: string;
  client_id: string;
  redirect_uri: string;
  clerk_state: string;
  nonce: string;
  code_challenge: string | null;
  auth_code: string | null;
  access_token: string | null;
  subject: string | null;
  name: string | null;
  picture: string | null;
  expires_at: number;
  consumed_at: number | null;
}

let signerCache: { jwk: string; signer: Signer } | undefined;

/** Present only when every WeChat and OIDC secret is set. Blank values count as unset. */
export function oidcSecrets(env: Env): Secrets | null {
  const appId = filled(env.WECHAT_APP_ID);
  const appSecret = filled(env.WECHAT_APP_SECRET);
  const clientId = filled(env.WECHAT_OIDC_CLIENT_ID);
  const clientSecret = filled(env.WECHAT_OIDC_CLIENT_SECRET);
  const privateJwk = filled(env.WECHAT_OIDC_PRIVATE_JWK);
  if (!appId || !appSecret || !clientId || !clientSecret || !privateJwk) return null;
  const listed = env.WECHAT_OIDC_REDIRECTS?.split(",").map((s) => s.trim()).filter(Boolean);
  return {
    appId,
    appSecret,
    clientId,
    clientSecret,
    privateJwk,
    redirects: new Set(listed?.length ? listed : DEFAULT_REDIRECTS),
  };
}

export async function handleWechatOidc(c: Ctx, fetchImpl: typeof fetch = fetch): Promise<Response> {
  const route = c.url.pathname.slice(PREFIX.length);
  const secrets = oidcSecrets(c.env);
  if (route === "/.well-known/openid-configuration") return discovery(c.url, secrets);
  if (route === "/jwks") return jwks(secrets);
  if (!secrets) return text(503, "WeChat sign-in isn't set up.");
  switch (route) {
    case "/authorize":
      return c.req.method === "GET" ? authorize(c, secrets) : text(405, "Method not allowed.");
    case "/callback":
      return c.req.method === "GET" ? callback(c, secrets, fetchImpl) : text(405, "Method not allowed.");
    case "/token":
      return c.req.method === "POST" ? token(c, secrets) : text(405, "Method not allowed.");
    case "/userinfo":
      return c.req.method === "GET" ? userinfo(c) : text(405, "Method not allowed.");
    default:
      return text(404, "Not found.");
  }
}

function discovery(url: URL, secrets: Secrets | null): Response {
  if (!secrets) return text(503, "WeChat sign-in isn't set up.");
  const issuer = `${url.origin}${PREFIX}`;
  return json(200, {
    issuer,
    authorization_endpoint: `${issuer}/authorize`,
    token_endpoint: `${issuer}/token`,
    userinfo_endpoint: `${issuer}/userinfo`,
    jwks_uri: `${issuer}/jwks`,
    response_types_supported: ["code"],
    subject_types_supported: ["public"],
    id_token_signing_alg_values_supported: ["ES256"],
    scopes_supported: ["openid", "profile"],
    token_endpoint_auth_methods_supported: ["client_secret_post", "client_secret_basic"],
    code_challenge_methods_supported: ["S256"],
  });
}

async function jwks(secrets: Secrets | null): Promise<Response> {
  const signer = secrets && (await signerFor(secrets.privateJwk));
  if (!signer) return text(503, "WeChat sign-in isn't set up.");
  return json(200, { keys: [signer.publicJwk] }, "public, max-age=300");
}

async function authorize(c: Ctx, secrets: Secrets): Promise<Response> {
  const q = c.url.searchParams;
  const redirect = q.get("redirect_uri") ?? "";
  if (!secrets.redirects.has(redirect)) return text(400, "This sign-in can't continue.");
  if (c.env.WECHAT_LIMITER) {
    const ip = c.req.headers.get("CF-Connecting-IP") ?? "unknown";
    if (!(await c.env.WECHAT_LIMITER.limit({ key: ip })).success) return text(429, "Too many sign-in attempts. Try again in a minute.");
  }
  const clientId = q.get("client_id");
  const state = q.get("state") ?? "";
  const nonce = q.get("nonce") ?? "";
  const challenge = q.get("code_challenge");
  const method = q.get("code_challenge_method");
  if (
    clientId !== secrets.clientId ||
    q.get("response_type") !== "code" ||
    !q.get("scope")?.split(" ").includes("openid") ||
    !bounded(state, 2048) ||
    !bounded(nonce, 512) ||
    (challenge !== null && method !== "S256") ||
    (challenge !== null && !bounded(challenge, 128))
  ) {
    return text(400, "This sign-in can't continue.");
  }
  const pending = randomId();
  await c.env.DB.prepare(
    `INSERT INTO wechat_oidc (state, client_id, redirect_uri, clerk_state, nonce, code_challenge, expires_at)
     VALUES (?, ?, ?, ?, ?, ?, ?)`,
  )
    .bind(pending, secrets.clientId, redirect, state, nonce, challenge, c.now + TXN_TTL_MS)
    .run();
  const callback = `${c.url.origin}${PREFIX}/callback`;
  const qr = new URL("https://open.weixin.qq.com/connect/qrconnect");
  qr.searchParams.set("appid", secrets.appId);
  qr.searchParams.set("redirect_uri", callback);
  qr.searchParams.set("response_type", "code");
  qr.searchParams.set("scope", "snsapi_login");
  qr.searchParams.set("state", pending);
  return redirectTo(`${qr}#wechat_redirect`);
}

async function callback(c: Ctx, secrets: Secrets, fetchImpl: typeof fetch): Promise<Response> {
  const state = c.url.searchParams.get("state") ?? "";
  if (!bounded(state, 128)) return text(400, "This sign-in can't continue.");
  const txn = await c.env.DB.prepare("SELECT * FROM wechat_oidc WHERE state = ? AND auth_code IS NULL AND expires_at > ?")
    .bind(state, c.now)
    .first<Txn>();
  if (!txn) return text(400, "This sign-in can't continue.");
  const code = c.url.searchParams.get("code");
  const profile = code ? await wechatProfile(c.env.DB, secrets, code, fetchImpl) : null;
  if (!profile) return redirectTo(withParams(txn.redirect_uri, { error: "access_denied", state: txn.clerk_state }));
  const authCode = randomId();
  const saved = await c.env.DB.prepare(
    `UPDATE wechat_oidc SET auth_code = ?, subject = ?, name = ?, picture = ?
     WHERE state = ? AND auth_code IS NULL AND expires_at > ?`,
  )
    .bind(authCode, profile.subject, profile.name, profile.picture, state, c.now)
    .run();
  if (!saved.meta.changes) return text(400, "This sign-in can't continue.");
  return redirectTo(withParams(txn.redirect_uri, { code: authCode, state: txn.clerk_state }));
}

async function token(c: Ctx, secrets: Secrets): Promise<Response> {
  const form = new URLSearchParams(new TextDecoder().decode(c.raw));
  const basic = basicAuth(c.req);
  const clientId = form.get("client_id") ?? basic?.id ?? "";
  const clientSecret = form.get("client_secret") ?? basic?.secret ?? "";
  if (basic && form.get("client_id") && form.get("client_id") !== basic.id) return oauthError(401, "invalid_client");
  if (clientId !== secrets.clientId || !(await same(clientSecret, secrets.clientSecret))) return oauthError(401, "invalid_client");
  if (form.get("grant_type") !== "authorization_code") return oauthError(400, "unsupported_grant_type");
  const code = form.get("code") ?? "";
  const redirect = form.get("redirect_uri") ?? "";
  const txn = await c.env.DB.prepare(
    "SELECT * FROM wechat_oidc WHERE auth_code = ? AND consumed_at IS NULL AND expires_at > ?",
  )
    .bind(code, c.now)
    .first<Txn>();
  if (!txn || txn.redirect_uri !== redirect || txn.client_id !== clientId || !txn.subject) return oauthError(400, "invalid_grant");
  const verifier = form.get("code_verifier");
  if (txn.code_challenge) {
    if (!verifier || verifier.length < 43 || verifier.length > 128 || !(await same(await s256(verifier), txn.code_challenge))) {
      return oauthError(400, "invalid_grant");
    }
  }
  const access = randomId();
  const accessExpires = c.now + TOKEN_TTL_S * 1000;
  const saved = await c.env.DB.prepare(
    `UPDATE wechat_oidc SET consumed_at = ?, access_token = ?, access_expires_at = ?
     WHERE state = ? AND consumed_at IS NULL`,
  )
    .bind(c.now, access, accessExpires, txn.state)
    .run();
  if (!saved.meta.changes) return oauthError(400, "invalid_grant");
  const signer = await signerFor(secrets.privateJwk);
  if (!signer) return text(503, "WeChat sign-in isn't set up.");
  const idToken = await signIdToken(signer, {
    iss: `${c.url.origin}${PREFIX}`,
    sub: txn.subject,
    aud: secrets.clientId,
    iat: Math.floor(c.now / 1000),
    exp: Math.floor(c.now / 1000) + TOKEN_TTL_S,
    nonce: txn.nonce,
    ...(txn.name ? { name: txn.name } : {}),
    ...(txn.picture ? { picture: txn.picture } : {}),
  });
  return json(200, { access_token: access, token_type: "Bearer", expires_in: TOKEN_TTL_S, id_token: idToken, scope: "openid profile" });
}

async function userinfo(c: Ctx): Promise<Response> {
  const header = c.req.headers.get("Authorization") ?? "";
  const access = header.startsWith("Bearer ") ? header.slice("Bearer ".length).trim() : "";
  if (!bounded(access, 128)) return oauthError(401, "invalid_token");
  const txn = await c.env.DB.prepare(
    `SELECT subject, name, picture FROM wechat_oidc
     WHERE access_token = ? AND consumed_at IS NOT NULL AND access_expires_at > ?`,
  )
    .bind(access, c.now)
    .first<{ subject: string; name: string | null; picture: string | null }>();
  if (!txn) return oauthError(401, "invalid_token");
  return json(200, { sub: txn.subject, ...(txn.name ? { name: txn.name } : {}), ...(txn.picture ? { picture: txn.picture } : {}) });
}

async function wechatProfile(db: D1Database, secrets: Secrets, code: string, fetchImpl: typeof fetch): Promise<Profile | null> {
  try {
    const tokenURL = new URL("https://api.weixin.qq.com/sns/oauth2/access_token");
    tokenURL.searchParams.set("appid", secrets.appId);
    tokenURL.searchParams.set("secret", secrets.appSecret);
    tokenURL.searchParams.set("code", code);
    tokenURL.searchParams.set("grant_type", "authorization_code");
    const token = await readJson(fetchImpl, tokenURL);
    if (!token || token.errcode) return null;
    const openid = wechatId(token.openid);
    if (!openid) return null;
    const info = await wechatUserInfo(token, openid, fetchImpl);
    const unionid = (info ? wechatId(info.unionid) : null) ?? wechatId(token.unionid);
    return {
      subject: await stableSubject(db, openid, unionid),
      name: info ? displayName(info.nickname) : null,
      picture: info ? httpsPicture(info.headimgurl) : null,
    };
  } catch (error) {
    console.error("wechat profile", { error: error instanceof Error ? error.name : "failed" });
    return null;
  }
}

async function wechatUserInfo(
  token: Record<string, unknown>,
  openid: string,
  fetchImpl: typeof fetch,
): Promise<Record<string, unknown> | null> {
  if (typeof token.access_token !== "string") return null;
  const infoURL = new URL("https://api.weixin.qq.com/sns/userinfo");
  infoURL.searchParams.set("access_token", token.access_token);
  infoURL.searchParams.set("openid", openid);
  infoURL.searchParams.set("lang", "zh_CN");
  const info = await readJson(fetchImpl, infoURL);
  if (!info || (typeof info.errcode === "number" && info.errcode !== 0)) return null;
  return info;
}

/** First subject issued for this openid. A failed profile must not switch it. */
async function stableSubject(db: D1Database, openid: string, unionid: string | null): Promise<string> {
  const chosen = unionid ?? openid;
  await db
    .prepare("INSERT INTO wechat_subject (openid, subject) VALUES (?, ?) ON CONFLICT(openid) DO NOTHING")
    .bind(openid, chosen)
    .run();
  const stored = await db.prepare("SELECT subject FROM wechat_subject WHERE openid = ?").bind(openid).first<{ subject: string }>();
  return stored?.subject || chosen;
}

async function readJson(fetchImpl: typeof fetch, url: URL): Promise<Record<string, unknown> | null> {
  const res = await fetchImpl(url, { redirect: "error", signal: AbortSignal.timeout(10_000) });
  if (!res.ok) return null;
  const body: unknown = await res.json();
  return body && typeof body === "object" ? (body as Record<string, unknown>) : null;
}

async function signerFor(jwkText: string): Promise<Signer | null> {
  if (signerCache?.jwk === jwkText) return signerCache.signer;
  let parsed: PrivateJwk;
  try {
    parsed = JSON.parse(jwkText) as PrivateJwk;
  } catch {
    return null;
  }
  if (parsed.kty !== "EC" || parsed.crv !== "P-256" || !parsed.d || !parsed.x || !parsed.y) return null;
  const kid = parsed.kid || "wechat";
  const key = await crypto.subtle.importKey("jwk", parsed, { name: "ECDSA", namedCurve: "P-256" }, false, ["sign"]);
  const signer = {
    key,
    kid,
    publicJwk: { kty: "EC", crv: "P-256", x: parsed.x, y: parsed.y, alg: "ES256", use: "sig", kid },
  };
  signerCache = { jwk: jwkText, signer };
  return signer;
}

async function signIdToken(signer: Signer, claims: Record<string, unknown>): Promise<string> {
  const signingInput = `${b64urlJson({ alg: "ES256", typ: "JWT", kid: signer.kid })}.${b64urlJson(claims)}`;
  const sig = new Uint8Array(
    await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, signer.key, new TextEncoder().encode(signingInput)),
  );
  return `${signingInput}.${b64url(sig)}`;
}

async function s256(verifier: string): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier));
  return b64url(new Uint8Array(digest));
}

async function same(a: string, b: string): Promise<boolean> {
  const encode = new TextEncoder();
  const [left, right] = await Promise.all([
    crypto.subtle.digest("SHA-256", encode.encode(a)),
    crypto.subtle.digest("SHA-256", encode.encode(b)),
  ]);
  return crypto.subtle.timingSafeEqual(new Uint8Array(left), new Uint8Array(right));
}

function basicAuth(req: Request): { id: string; secret: string } | null {
  const header = req.headers.get("Authorization") ?? "";
  if (!header.startsWith("Basic ")) return null;
  try {
    const decoded = atob(header.slice("Basic ".length));
    const split = decoded.indexOf(":");
    if (split < 0) return null;
    return { id: decodeURIComponent(decoded.slice(0, split)), secret: decodeURIComponent(decoded.slice(split + 1)) };
  } catch {
    return null;
  }
}

function wechatId(value: unknown): string | null {
  if (typeof value !== "string" || value.length < 1 || value.length > 128) return null;
  if (/[\s\u0000-\u001f]/.test(value)) return null;
  return value;
}

function displayName(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const name = value.replace(/[\u0000-\u001f]/g, "").trim().slice(0, 64);
  return name || null;
}

function httpsPicture(value: unknown): string | null {
  if (typeof value !== "string" || value.length > 2048) return null;
  const raw = value.startsWith("http://") ? `https://${value.slice("http://".length)}` : value;
  try {
    const url = new URL(raw);
    return url.protocol === "https:" ? url.toString() : null;
  } catch {
    return null;
  }
}

function withParams(redirect: string, params: Record<string, string>): string {
  const url = new URL(redirect);
  for (const [key, value] of Object.entries(params)) url.searchParams.set(key, value);
  return url.toString();
}

function filled(value: string | undefined): string | null {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

function bounded(value: string, max: number): boolean {
  return value.length > 0 && value.length <= max;
}

function randomId(): string {
  const bytes = new Uint8Array(32);
  crypto.getRandomValues(bytes);
  return b64url(bytes);
}

function b64urlJson(value: unknown): string {
  return b64url(new TextEncoder().encode(JSON.stringify(value)));
}

function b64url(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replaceAll("=", "");
}

function redirectTo(location: string): Response {
  return new Response(null, { status: 302, headers: { Location: location, "Cache-Control": "no-store" } });
}

function text(status: number, message: string): Response {
  return new Response(message, { status, headers: { "Content-Type": "text/plain; charset=utf-8", "Cache-Control": "no-store" } });
}

function json(status: number, body: unknown, cache = "no-store"): Response {
  return Response.json(body, { status, headers: { "Cache-Control": cache } });
}

function oauthError(status: number, error: string): Response {
  return json(status, { error });
}
