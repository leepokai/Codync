// WeChat's QR login, bridged to the OIDC connection Clerk is configured with.

import { createScheduledController, SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import worker from "../src/index";
import type { Ctx } from "../src/routes/common";
import { handleWechatOidc } from "../src/wechat-oidc";
import { env, ORIGIN } from "./helpers";
import { b64url, unb64url } from "./ref";

const REDIRECT = "https://clerk.codync.dev/v1/oauth_callback";
const DEV_REDIRECT = "https://sunny-mollusk-8651.clerk.accounts.dev/v1/oauth_callback";

describe("WeChat sign-in", () => {
  it("publishes a discovery document and a public signing key", async () => {
    const doc = await json<Record<string, unknown>>(await SELF.fetch(`${ORIGIN}/v1/wechat/oidc/.well-known/openid-configuration`));
    expect(doc).toMatchObject({
      issuer: `${ORIGIN}/v1/wechat/oidc`,
      authorization_endpoint: `${ORIGIN}/v1/wechat/oidc/authorize`,
      token_endpoint: `${ORIGIN}/v1/wechat/oidc/token`,
      id_token_signing_alg_values_supported: ["ES256"],
      code_challenge_methods_supported: ["S256"],
    });
    const jwks = await json<{ keys: (JsonWebKey & { kid?: string })[] }>(await SELF.fetch(`${ORIGIN}/v1/wechat/oidc/jwks`));
    expect(jwks.keys).toHaveLength(1);
    expect(jwks.keys[0]).toMatchObject({ kty: "EC", crv: "P-256", alg: "ES256", use: "sig", kid: "wechat-test" });
    expect(jwks.keys[0].d).toBeUndefined();
  });

  it("stays off until the WeChat and OIDC secrets are all set", async () => {
    const url = new URL(`${ORIGIN}/v1/wechat/oidc/authorize`);
    const res = await handleWechatOidc(ctx(url, { WECHAT_APP_ID: "  " }));
    expect(res.status).toBe(503);
  });

  it("sends the browser to WeChat's QR page and refuses any other redirect", async () => {
    const ok = await start({ nonce: "n-dev", redirect: DEV_REDIRECT });
    const location = ok.headers.get("Location") ?? "";
    const qr = new URL(location);
    expect(qr.origin).toBe("https://open.weixin.qq.com");
    expect(qr.pathname).toBe("/connect/qrconnect");
    expect(qr.searchParams.get("appid")).toBe(env.WECHAT_APP_ID);
    expect(qr.searchParams.get("scope")).toBe("snsapi_login");
    expect(qr.searchParams.get("redirect_uri")).toBe(`${ORIGIN}/v1/wechat/oidc/callback`);
    expect(qr.hash).toBe("#wechat_redirect");
    expect(location).not.toContain(env.WECHAT_APP_SECRET);

    const evil = await start({ nonce: "n-evil", redirect: "https://evil.example/callback" });
    expect(evil.status).toBe(400);
    expect(evil.headers.get("Location")).toBeNull();
    expect(await evil.text()).not.toContain("evil.example");
  });

  it("signs an id_token for the unionid and serves the profile once", async () => {
    const nonce = "nonce-union";
    const started = await start({ nonce });
    const state = new URL(started.headers.get("Location") ?? "").searchParams.get("state");
    const wechat = scripted(
      { access_token: "wx-access", openid: "openid-1", unionid: "union-1" },
      { nickname: "小\n林", headimgurl: "http://thirdwx.qlogo.cn/mmopen/abc" },
    );
    const back = await handleWechatOidc(ctx(callbackUrl(state, "wx-code")), wechat.fetch);
    expect(back.status).toBe(302);
    const returned = new URL(back.headers.get("Location") ?? "");
    expect(returned.origin + returned.pathname).toBe(REDIRECT);
    expect(returned.searchParams.get("state")).toBe("clerk-state");
    const code = returned.searchParams.get("code") ?? "";
    expect(wechat.urls[0]).toContain(`secret=${env.WECHAT_APP_SECRET}`);
    expect(wechat.urls[0]).toContain("code=wx-code");
    expect(wechat.urls[0]).not.toContain(REDIRECT);

    const wrong = await redeem(code, { client_secret: "nope" });
    expect(wrong.status).toBe(401);
    const first = await redeem(code);
    expect(first.status).toBe(200);
    const body = await json<{ id_token: string; access_token: string }>(first);
    const claims = await verifiedClaims(body.id_token);
    expect(claims).toMatchObject({
      iss: `${ORIGIN}/v1/wechat/oidc`,
      sub: "union-1",
      aud: env.WECHAT_OIDC_CLIENT_ID,
      nonce,
      name: "小林",
      picture: "https://thirdwx.qlogo.cn/mmopen/abc",
    });
    const profile = await SELF.fetch(`${ORIGIN}/v1/wechat/oidc/userinfo`, {
      headers: { Authorization: `Bearer ${body.access_token}` },
    });
    expect(await json(profile)).toEqual({ sub: "union-1", name: "小林", picture: "https://thirdwx.qlogo.cn/mmopen/abc" });
    expect((await redeem(code)).status).toBe(400);
  });

  it("uses the openid when WeChat returns no unionid, and drops an unsafe avatar", async () => {
    const started = await start({ nonce: "nonce-open" });
    const state = new URL(started.headers.get("Location") ?? "").searchParams.get("state");
    const back = await handleWechatOidc(
      ctx(callbackUrl(state, "wx-open")),
      scripted({ access_token: "wx-access", openid: "openid-only" }, { nickname: "Ada", headimgurl: "javascript:alert(1)" }).fetch,
    );
    const code = new URL(back.headers.get("Location") ?? "").searchParams.get("code") ?? "";
    const token = await redeem(code, { basic: true });
    expect(token.status).toBe(200);
    const claims = await verifiedClaims((await json<{ id_token: string }>(token)).id_token);
    expect(claims.sub).toBe("openid-only");
    expect(claims.name).toBe("Ada");
    expect(claims.picture).toBeUndefined();
  });

  it("requires the PKCE verifier that matches the challenge", async () => {
    const verifier = "a".repeat(50);
    const started = await start({ nonce: "nonce-pkce", challenge: await s256(verifier) });
    const state = new URL(started.headers.get("Location") ?? "").searchParams.get("state");
    const back = await handleWechatOidc(
      ctx(callbackUrl(state, "wx-pkce")),
      scripted({ access_token: "wx-access", openid: "openid-pkce", unionid: "union-pkce" }, {}).fetch,
    );
    const code = new URL(back.headers.get("Location") ?? "").searchParams.get("code") ?? "";
    expect((await redeem(code)).status).toBe(400);
    expect((await redeem(code, { code_verifier: `${verifier}no` })).status).toBe(400);
    const ok = await redeem(code, { code_verifier: verifier });
    expect(ok.status).toBe(200);
    expect((await verifiedClaims((await json<{ id_token: string }>(ok)).id_token)).sub).toBe("union-pkce");
  });

  it("returns the browser to Clerk when WeChat refuses or the person cancels", async () => {
    const cancelled = await start({ nonce: "nonce-cancel" });
    const cancelState = new URL(cancelled.headers.get("Location") ?? "").searchParams.get("state");
    const cancel = await handleWechatOidc(ctx(callbackUrl(cancelState, null)));
    expect(new URL(cancel.headers.get("Location") ?? "").searchParams.get("error")).toBe("access_denied");

    const refused = await start({ nonce: "nonce-refused" });
    const refusedState = new URL(refused.headers.get("Location") ?? "").searchParams.get("state");
    const refusal = await handleWechatOidc(
      ctx(callbackUrl(refusedState, "bad-code")),
      scripted({ errcode: 40029, errmsg: "invalid code" }, {}).fetch,
    );
    expect(new URL(refusal.headers.get("Location") ?? "").searchParams.get("error")).toBe("access_denied");
    expect(refusal.headers.get("Location")).not.toContain("invalid code");
  });

  it("keeps the unionid when a later profile request fails", async () => {
    const token = { access_token: "wx-access", openid: "openid-stable" };
    expect(await issuedSubject(token, { unionid: "union-stable", nickname: "小林" })).toBe("union-stable");
    expect(await issuedSubject(token, { errcode: 503, errmsg: "busy", unionid: "other-union" })).toBe("union-stable");
    expect(await issuedSubject(token, {}, 503)).toBe("union-stable");
  });

  it("does not take a subject from a failed profile", async () => {
    const token = { access_token: "wx-access", openid: "openid-first-fail" };
    expect(await issuedSubject(token, { errcode: 40001, unionid: "decoy-union" })).toBe("openid-first-fail");
    expect(await issuedSubject(token, { unionid: "union-later" })).toBe("openid-first-fail");
  });

  it("honors the access token for an hour after the sign-in code has expired", async () => {
    const started = await start({ nonce: "nonce-hour" });
    const state = new URL(started.headers.get("Location") ?? "").searchParams.get("state");
    const back = await handleWechatOidc(
      ctx(callbackUrl(state, "wx-hour")),
      scripted({ access_token: "wx-access", openid: "openid-hour", unionid: "union-hour" }, {}).fetch,
    );
    const code = new URL(back.headers.get("Location") ?? "").searchParams.get("code") ?? "";
    const redeemed = await redeem(code);
    expect(redeemed.status).toBe(200);
    const body = await json<{ access_token: string; expires_in: number }>(redeemed);
    expect(body.expires_in).toBe(60 * 60);
    const row = await env.DB.prepare("SELECT access_expires_at AS deadline FROM wechat_oidc WHERE access_token = ?")
      .bind(body.access_token)
      .first<{ deadline: number }>();
    expect(row?.deadline).toBeGreaterThan(Date.now() + 59 * 60 * 1000);

    await env.DB.prepare("UPDATE wechat_oidc SET expires_at = ? WHERE access_token = ?").bind(Date.now() - 1, body.access_token).run();
    const profile = await SELF.fetch(`${ORIGIN}/v1/wechat/oidc/userinfo`, {
      headers: { Authorization: `Bearer ${body.access_token}` },
    });
    expect(profile.status).toBe(200);
    expect(await json(profile)).toMatchObject({ sub: "union-hour" });
    await worker.scheduled(createScheduledController({ cron: "*/15 * * * *" }), env);
    expect(await env.DB.prepare("SELECT access_token FROM wechat_oidc WHERE access_token = ?").bind(body.access_token).first()).toEqual({
      access_token: body.access_token,
    });

    await env.DB.prepare("UPDATE wechat_oidc SET access_expires_at = ? WHERE access_token = ?").bind(Date.now() - 1, body.access_token).run();
    const expired = await SELF.fetch(`${ORIGIN}/v1/wechat/oidc/userinfo`, {
      headers: { Authorization: `Bearer ${body.access_token}` },
    });
    expect(expired.status).toBe(401);
    await worker.scheduled(createScheduledController({ cron: "*/15 * * * *" }), env);
    expect(await env.DB.prepare("SELECT access_token FROM wechat_oidc WHERE access_token = ?").bind(body.access_token).first()).toBeNull();
  });

  it("rejects an expired code and the sweep drops expired rows", async () => {
    const started = await start({ nonce: "nonce-old" });
    const state = new URL(started.headers.get("Location") ?? "").searchParams.get("state");
    const back = await handleWechatOidc(
      ctx(callbackUrl(state, "wx-old")),
      scripted({ access_token: "wx-access", openid: "openid-old", unionid: "union-old" }, {}).fetch,
    );
    const code = new URL(back.headers.get("Location") ?? "").searchParams.get("code") ?? "";
    await env.DB.prepare("UPDATE wechat_oidc SET expires_at = ? WHERE auth_code = ?").bind(Date.now() - 1, code).run();
    expect((await redeem(code)).status).toBe(400);

    const live = `live-${crypto.randomUUID()}`;
    await env.DB.prepare(
      "INSERT INTO wechat_oidc (state, client_id, redirect_uri, clerk_state, nonce, expires_at) VALUES (?, 'c', ?, 's', 'n', ?)",
    )
      .bind(live, REDIRECT, Date.now() + 60_000)
      .run();
    await worker.scheduled(createScheduledController({ cron: "*/15 * * * *" }), env);
    const expired = await env.DB.prepare("SELECT state FROM wechat_oidc WHERE auth_code = ?").bind(code).first();
    const kept = await env.DB.prepare("SELECT state FROM wechat_oidc WHERE state = ?").bind(live).first();
    expect(expired).toBeNull();
    expect(kept).toEqual({ state: live });
  });
});

async function start(o: { nonce: string; redirect?: string; challenge?: string }): Promise<Response> {
  const q = new URLSearchParams({
    client_id: env.WECHAT_OIDC_CLIENT_ID ?? "",
    redirect_uri: o.redirect ?? REDIRECT,
    response_type: "code",
    scope: "openid profile",
    state: "clerk-state",
    nonce: o.nonce,
  });
  if (o.challenge) {
    q.set("code_challenge", o.challenge);
    q.set("code_challenge_method", "S256");
  }
  return SELF.fetch(`${ORIGIN}/v1/wechat/oidc/authorize?${q}`, { redirect: "manual" });
}

function callbackUrl(state: string | null, code: string | null): URL {
  const url = new URL(`${ORIGIN}/v1/wechat/oidc/callback`);
  if (state) url.searchParams.set("state", state);
  if (code) url.searchParams.set("code", code);
  return url;
}

async function redeem(code: string, extra: { client_secret?: string; code_verifier?: string; basic?: boolean } = {}): Promise<Response> {
  const body = new URLSearchParams({
    grant_type: "authorization_code",
    code,
    redirect_uri: REDIRECT,
    client_id: env.WECHAT_OIDC_CLIENT_ID ?? "",
  });
  const headers: Record<string, string> = { "Content-Type": "application/x-www-form-urlencoded" };
  if (extra.basic) {
    headers.Authorization = `Basic ${btoa(`${env.WECHAT_OIDC_CLIENT_ID}:${env.WECHAT_OIDC_CLIENT_SECRET}`)}`;
  } else {
    body.set("client_secret", extra.client_secret ?? env.WECHAT_OIDC_CLIENT_SECRET ?? "");
  }
  if (extra.code_verifier) body.set("code_verifier", extra.code_verifier);
  return SELF.fetch(`${ORIGIN}/v1/wechat/oidc/token`, { method: "POST", headers, body });
}

function scripted(
  token: Record<string, unknown>,
  user: Record<string, unknown>,
  userStatus = 200,
): { fetch: typeof fetch; urls: string[] } {
  const urls: string[] = [];
  const fetchImpl: typeof fetch = async (input) => {
    const url = String(input);
    urls.push(url);
    const profile = url.includes("/sns/userinfo");
    return new Response(JSON.stringify(profile ? user : token), {
      status: profile ? userStatus : 200,
      headers: { "Content-Type": "application/json" },
    });
  };
  return { fetch: fetchImpl, urls };
}

async function issuedSubject(token: Record<string, unknown>, user: Record<string, unknown>, userStatus = 200): Promise<string> {
  const started = await start({ nonce: `nonce-${crypto.randomUUID()}` });
  const state = new URL(started.headers.get("Location") ?? "").searchParams.get("state");
  const back = await handleWechatOidc(ctx(callbackUrl(state, `wx-${crypto.randomUUID()}`)), scripted(token, user, userStatus).fetch);
  const code = new URL(back.headers.get("Location") ?? "").searchParams.get("code") ?? "";
  const redeemed = await redeem(code);
  expect(redeemed.status).toBe(200);
  return String((await verifiedClaims((await json<{ id_token: string }>(redeemed)).id_token)).sub);
}

function ctx(url: URL, overrides: Partial<typeof env> = {}): Ctx {
  return {
    req: new Request(url),
    env: { ...env, ...overrides },
    exec: {} as ExecutionContext,
    url,
    raw: new Uint8Array(),
    now: Date.now(),
  };
}

async function s256(verifier: string): Promise<string> {
  return b64url(new Uint8Array(await crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier))));
}

async function json<T>(res: Response): Promise<T> {
  return (await res.json()) as T;
}

async function verifiedClaims(jwt: string): Promise<Record<string, unknown>> {
  const [head, payload, sig] = jwt.split(".");
  const jwks = await json<{ keys: JsonWebKey[] }>(await SELF.fetch(`${ORIGIN}/v1/wechat/oidc/jwks`));
  const key = await crypto.subtle.importKey("jwk", jwks.keys[0], { name: "ECDSA", namedCurve: "P-256" }, false, ["verify"]);
  const ok = await crypto.subtle.verify(
    { name: "ECDSA", hash: "SHA-256" },
    key,
    unb64url(sig ?? ""),
    new TextEncoder().encode(`${head}.${payload}`),
  );
  expect(ok).toBe(true);
  expect(JSON.parse(new TextDecoder().decode(unb64url(head))).kid).toBe("wechat-test");
  return JSON.parse(new TextDecoder().decode(unb64url(payload)));
}
