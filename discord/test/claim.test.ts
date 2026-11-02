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
  SESSION_SECRET: "session", MENTIONS_SECRET: "notes", GUILD_ID: "guild", CONTRIBUTOR_ROLE_ID: "contributor", REPORTER_ROLE_ID: "reporter",
  STAFF_CHANNEL_ID: "staff", MILESTONES_CHANNEL_ID: "milestones", STAR_GOAL: "1000", DOWNLOAD_GOAL: "10000", REPO: "leepokai/Codync",
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
links.set("issue:5", "{}"); // already in the forum (issues.test.ts covers the post)
assert.equal(await hook("issues", issue), 204);
assert.deepEqual(calls, []);

calls = stub({ member: false });
assert.equal(await hook("pull_request", pr(true)), 204);
assert.deepEqual(calls, []);

// Release notes: linked logins become mentions, only for notes signed with the mentions secret.
links.clear();
links.set("github:7", "42");
globalThis.fetch = (async (input: string) => {
  const login = String(input).match(/\/users\/([^/]+)$/)?.[1];
  if (login === "octo") return Response.json({ id: 7 });
  if (login === "other") return Response.json({ id: 8 });
  if (login) return new Response(null, { status: 404 });
  throw new Error(`unexpected ${input}`);
}) as typeof fetch;
const notes = async (body: string, secret = "notes") => {
  const res = await worker.fetch(new Request("https://discord.test/mentions", {
    method: "POST", body, headers: { "X-Signature": await sign(secret, body) },
  }), env);
  return res.status === 200 ? res.json() : res.status;
};
assert.equal(await notes("Thanks @octo", "wrong"), 401);
assert.deepEqual(await notes("- Fast mode. Thanks @octo (#101).\n@Octo, @other, @nobody, a@octo.dev, `@octo_x`"), {
  text: "- Fast mode. Thanks <@42> (#101).\n<@42>, @other, @nobody, a@octo.dev, `@octo_x`",
  users: ["42"],
});
// Milestones: each 100th star and 1,000th download posts once in #milestones without pinging anyone, with progress
// toward the goal until it's reached.
const posts: string[] = [];
let up = true;
let releases: object[][] = [];
globalThis.fetch = (async (input: string, init?: RequestInit) => {
  const url = String(input);
  const page = url.match(/^https:\/\/api\.github\.com\/repos\/leepokai\/Codync\/releases\?per_page=100&page=(\d+)$/)?.[1];
  if (page) return Response.json(releases[Number(page) - 1] ?? []);
  if (url.startsWith("https://api.github.com/repos/leepokai/Codync/issues?")) return Response.json([]);
  assert.equal(url, "https://discord.com/api/v10/channels/milestones/messages");
  if (!up) return new Response("down", { status: 503 });
  const body = JSON.parse(String(init?.body));
  assert.deepEqual(body.allowed_mentions, { parse: [] });
  posts.push(body.content);
  return Response.json({});
}) as typeof fetch;
const star = (count: number, action = "created") => hook("star", { action, repository: { stargazers_count: count } });
assert.equal(await star(99), 204);
assert.equal(await star(402, "deleted"), 204);
assert.deepEqual(posts, []);
up = false;
assert.equal(await star(400), 204);
assert.equal(links.get("stars:milestone"), undefined);
up = true;
for (const count of [402, 401, 400, 450, 1000, 1000, 1104]) assert.equal(await star(count), 204);
const thanks = "Thanks to everyone who starred [leepokai/Codync](https://github.com/leepokai/Codync) 💛\n\n📣 ";
assert.equal(posts.length, 3);
assert.ok(posts[0].startsWith(`⭐ **Codync just hit 400 stars on GitHub!**\n\`████████░░░░░░░░░░░░\` 400 / 1,000\n${thanks}Help us reach 1,000: tell people about Codync on [X](https://x.com/intent/post?text=Codync%3A%20`));
assert.ok(posts[1].startsWith(`🎉 **Codync hit 1,000 stars on GitHub, goal reached!**\n${thanks}Keep spreading the word on [X](`));
assert.ok(posts[2].startsWith(`⭐ **Codync just hit 1,100 stars on GitHub!**\n${thanks}Keep spreading the word on [X](`));
assert.match(posts[0], /\[Threads\]\(.+\) · \[Reddit\]\(.+url=https%3A%2F%2Fgithub.com%2Fleepokai%2FCodync.+\) · \[LinkedIn\]\(.+\), or wherever you hang out\.$/);
assert.equal(links.get("stars:milestone"), "1100");

// Downloads count installers across every release page, not updater files, checksums or host tarballs.
posts.length = 0;
const asset = (name: string, download_count: number) => ({ name, download_count });
const release = (...assets: object[]) => ({ assets });
releases = [
  Array.from({ length: 100 }, (_, i) => release(asset("codync-macos.dmg", i === 0 ? 900 : 0), asset("latest-mac.yml", 5000))),
  [release(asset("codync-windows-x64.exe", 80), asset("codync-windows-x64.exe.blockmap", 999), asset("codync-linux-x86_64.AppImage", 15),
    asset("codync-linux-arm64.tar.gz", 4), asset("codync-host-linux-x86_64.tar.gz", 999), asset("codync-macos.zip", 999))],
];
const cron = () => worker.scheduled({} as ScheduledController, env);
await cron();
assert.deepEqual(posts, []);
releases[1].push(release(asset("codync-linux-x86_64.deb", 1)));
await cron();
await cron();
assert.equal(posts.length, 1);
assert.ok(posts[0].startsWith("📦 **Codync just hit 1,000 downloads!**\n`██░░░░░░░░░░░░░░░░░░` 1,000 / 10,000\nThanks to everyone who gave it a try 💛\n\n📣 Help us reach 10,000:"));
assert.equal(links.get("downloads:milestone"), "1000");
console.log("claim tests ok");
