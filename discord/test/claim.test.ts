// Link and webhook flows against stubbed Discord, GitHub and KV: signing in adds the member to the server, a
// verified GitHub connection (or a GitHub sign-in) earns roles, the link is remembered, and later merged pull
// requests and opened issues earn roles through the webhook.
import assert from "node:assert/strict";
import { sign } from "../src/hmac.ts";
import worker from "../src/index.ts";

const links = new Map<string, string>();
const env = {
  DISCORD_CLIENT_ID: "app", DISCORD_CLIENT_SECRET: "secret", DISCORD_BOT_TOKEN: "bot",
  GITHUB_CLIENT_ID: "ghapp", GITHUB_CLIENT_SECRET: "ghsecret", GITHUB_TOKEN: "gh", GITHUB_WEBHOOK_SECRET: "hook",
  SESSION_SECRET: "session", GUILD_ID: "guild", CONTRIBUTOR_ROLE_ID: "contributor", REPORTER_ROLE_ID: "reporter",
  STAFF_CHANNEL_ID: "staff", REPO: "leepokai/Codync",
  LINKS: { get: async (k: string) => links.get(k) ?? null, put: async (k: string, v: string) => void links.set(k, v) },
} as never;

const github = { type: "github", id: "7", verified: true };

/** Stubs every upstream call and records the ones with effects: join, role, notify. */
function stub({ connections = [github] as object[], prs = 0, issues = 0, held = [] as string[], member = true } = {}) {
  const calls: string[] = [];
  globalThis.fetch = (async (input: string, init?: RequestInit) => {
    const url = String(input);
    const method = init?.method ?? "GET";
    if (url.endsWith("/oauth2/token")) return Response.json({ access_token: "user" });
    if (url.endsWith("/users/@me")) return Response.json({ id: "42" });
    if (url.endsWith("/users/@me/connections")) return Response.json(connections);
    if (url === "https://github.com/login/oauth/access_token") {
      const good = new URLSearchParams(String(init?.body)).get("code") === "good";
      return Response.json(good ? { access_token: "gho" } : { error: "bad_verification_code" });
    }
    if (url === "https://api.github.com/user") return Response.json({ id: 7 });
    if (url.endsWith("/user/7")) return Response.json({ login: "octo" });
    if (url.includes("/search/issues")) {
      const q = decodeURIComponent(url);
      assert.match(q, /repo:leepokai\/Codync .* author:octo/);
      return Response.json({ total_count: q.includes("is:pr is:merged") ? prs : issues });
    }
    if (url.endsWith("/guilds/guild/members/42") && method === "PUT") {
      assert.equal(JSON.parse(String(init?.body)).access_token, "user");
      calls.push("join");
      return new Response(null, { status: 201 });
    }
    if (url.endsWith("/guilds/guild/members/42")) return member ? Response.json({ roles: held }) : new Response(null, { status: 404 });
    if (url.includes("/guilds/guild/members/42/roles/")) {
      assert.equal(method, "PUT");
      calls.push(`role ${url.split("/").pop()}`);
      return new Response(null, { status: 204 });
    }
    if (url.endsWith("/channels/staff/messages")) {
      calls.push(`notify ${JSON.parse(String(init?.body)).content}`);
      return Response.json({});
    }
    throw new Error(`unexpected ${method} ${url}`);
  }) as typeof fetch;
  return calls;
}

const get = (path: string, cookie = "") => worker.fetch(new Request(`https://discord.test${path}`, { headers: { Cookie: cookie } }), env);
const discordCallback = async (state = "s", cookie = "s") => {
  const res = await get(`/callback?code=c&state=${state}`, `state=${cookie}`);
  return { res, text: await res.text() };
};

// Sign-in asks Discord for the connections and to add the member to the server.
const start = await get("/");
assert.equal(start.status, 302);
const authorize = new URL(start.headers.get("Location")!);
assert.equal(authorize.searchParams.get("scope"), "identify connections guilds.join");
assert.equal(authorize.searchParams.get("state"), start.headers.get("Set-Cookie")!.match(/state=([^;]+)/)![1]);

let calls = stub({ prs: 1 });
assert.equal((await discordCallback("s", "forged")).res.status, 400);
assert.deepEqual(calls, []);

// A verified connection with a merged PR: joined, Contributor, remembered, staff told.
assert.match((await discordCallback()).text, /You're a Contributor</);
assert.deepEqual(calls, ["join", "role contributor", "notify <@42> linked GitHub [octo](https://github.com/octo) → Contributor"]);
assert.equal(links.get("github:7"), "42");

// Opening the link again changes nothing and stays quiet.
calls = stub({ prs: 1, held: ["contributor"] });
assert.match((await discordCallback()).text, /You're a Contributor</);
assert.deepEqual(calls, ["join"]);

calls = stub({ prs: 3, issues: 1, held: ["contributor"] });
assert.match((await discordCallback()).text, /You're a Contributor and Reporter/);
assert.deepEqual(calls, ["join", "role reporter", "notify <@42> linked GitHub [octo](https://github.com/octo) → Reporter"]);

// Nothing earned yet: still joined and linked.
links.clear();
calls = stub();
assert.match((await discordCallback()).text, /You're in, @octo/);
assert.deepEqual(calls, ["join", "notify <@42> linked GitHub [octo](https://github.com/octo)"]);
assert.equal(links.get("github:7"), "42");

// No verified connection: joined, then on to GitHub sign-in with a signed session.
links.clear();
calls = stub({ connections: [{ ...github, verified: false }], issues: 1 });
const { res: toGitHub } = await discordCallback();
assert.equal(toGitHub.status, 302);
assert.deepEqual(calls, ["join"]);
const login = new URL(toGitHub.headers.get("Location")!);
assert.equal(login.origin + login.pathname, "https://github.com/login/oauth/authorize");
const session = toGitHub.headers.get("Set-Cookie")!.match(/session=([^;]+)/)![1];
const [state, discordId, signature] = session.split(".");
assert.equal(login.searchParams.get("state"), state);
assert.equal(discordId, "42");

const githubCallback = async (code: string, cookie: string) => {
  const res = await get(`/github/callback?code=${code}&state=${state}`, `session=${cookie}`);
  return { status: res.status, text: await res.text() };
};
assert.equal((await githubCallback("good", `${state}.99.${signature}`)).status, 400);
assert.equal((await githubCallback("good", `${state}.42.${"0".repeat(64)}`)).status, 400);
assert.equal((await githubCallback("used", session)).status, 400);
assert.deepEqual(calls, ["join"]);
assert.match((await githubCallback("good", session)).text, /You're a Reporter</);
assert.deepEqual(calls, ["join", "role reporter", "notify <@42> linked GitHub [octo](https://github.com/octo) → Reporter"]);
assert.equal(links.get("github:7"), "42");

// Webhook: signed by GitHub, and only for linked accounts.
const hook = async (event: string, payload: object, secret = "hook") => {
  const body = JSON.stringify(payload);
  const res = await worker.fetch(new Request("https://discord.test/webhook", {
    method: "POST",
    body,
    headers: { "X-GitHub-Event": event, "X-Hub-Signature-256": `sha256=${await sign(secret, body)}` },
  }), env);
  return res.status;
};
const pr = (merged: boolean, id = 7) => ({
  action: "closed",
  pull_request: { number: 12, merged, html_url: "https://github.com/leepokai/Codync/pull/12", user: { id, login: "octo" } },
});

calls = stub();
assert.equal(await hook("pull_request", pr(true), "wrong"), 401);
assert.equal(await hook("ping", { zen: "hi" }), 204);
assert.equal(await hook("pull_request", pr(false)), 204);
assert.equal(await hook("pull_request", pr(true, 8)), 204);
assert.deepEqual(calls, []);
assert.equal(await hook("pull_request", pr(true)), 204);
assert.deepEqual(calls, ["role contributor", "notify <@42> ([octo](https://github.com/octo)) earned Contributor with [PR #12](https://github.com/leepokai/Codync/pull/12)"]);

calls = stub({ held: ["reporter"] });
const issue = { action: "opened", issue: { number: 5, html_url: "https://github.com/leepokai/Codync/issues/5", user: { id: 7, login: "octo" } } };
assert.equal(await hook("issues", issue), 204);
assert.deepEqual(calls, []);

calls = stub({ member: false });
assert.equal(await hook("pull_request", pr(true)), 204);
assert.deepEqual(calls, []);
console.log("claim tests ok");
