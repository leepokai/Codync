// GitHub issues → Discord forum posts, against stubbed Discord, GitHub and KV: an opened issue gets one post in the
// right forum, closing and reopening update it, and the daily run posts open issues that have none yet.
import assert from "node:assert/strict";
import { sign } from "../src/hmac.ts";
import worker from "../src/index.ts";

const links = new Map<string, string>();
const env = {
  DISCORD_BOT_TOKEN: "bot", GITHUB_TOKEN: "gh", GITHUB_WEBHOOK_SECRET: "hook", REPO: "leepokai/Codync",
  BUG_FORUM_ID: "bugs", FEATURE_FORUM_ID: "features", MILESTONES_CHANNEL_ID: "milestones", STAR_GOAL: "1000", DOWNLOAD_GOAL: "10000",
  LINKS: { get: async (k: string) => links.get(k) ?? null, put: async (k: string, v: string) => void links.set(k, v) },
} as never;

const tags = {
  bugs: [{ id: "gh-b", name: "GitHub" }, { id: "fixed", name: "Fixed" }],
  features: [{ id: "gh-f", name: "GitHub" }, { id: "shipped", name: "Shipped" }],
};
let open: object[] = [];
let calls: string[] = [];
let threadTags: string[] = [];
globalThis.fetch = (async (input: string, init?: RequestInit) => {
  const url = String(input);
  const method = init?.method ?? "GET";
  const body = init?.body ? JSON.parse(String(init.body)) : null;
  if (url.startsWith("https://api.github.com/repos/leepokai/Codync/releases?")) return Response.json([]);
  if (url.startsWith("https://api.github.com/repos/leepokai/Codync/issues?state=open")) return Response.json(open);
  const forum = url.match(/\/channels\/(bugs|features)(\/threads)?$/);
  if (forum && !forum[2]) return Response.json({ available_tags: tags[forum[1] as "bugs"] });
  if (forum) {
    calls.push(`post ${forum[1]} "${body.name}" ${body.applied_tags} ${body.message.content.split("\n")[0]}`);
    return Response.json({ id: `t${body.name.match(/#(\d+)/)[1]}` });
  }
  const thread = url.match(/\/channels\/(t\d+)(\/messages)?$/);
  if (thread?.[2]) { calls.push(`say ${thread[1]} ${body.content}`); return Response.json({}); }
  if (thread && method === "GET") return Response.json({ applied_tags: threadTags });
  if (thread && method === "PATCH") { threadTags = body.applied_tags; calls.push(`set ${thread[1]} [${body.applied_tags}] archived=${body.archived}`); return Response.json({}); }
  if (url.includes("/guilds/")) throw new Error("no roles for unlinked accounts");
  throw new Error(`unexpected ${method} ${url}`);
}) as typeof fetch;

const hook = async (action: string, issue: object) => {
  const body = JSON.stringify({ action, issue });
  const res = await worker.fetch(new Request("https://discord.test/webhook", {
    method: "POST", body, headers: { "X-GitHub-Event": "issues", "X-Hub-Signature-256": `sha256=${await sign("hook", body)}` },
  }), env);
  return res.status;
};
const issue = (number: number, title: string, extra: object = {}) => ({
  number, title, body: "<!-- template -->\nSteps to reproduce", html_url: `https://github.com/leepokai/Codync/issues/${number}`,
  labels: [], user: { id: 9, login: "octo" }, ...extra,
});

// Opened: one post in the matching forum, tagged GitHub; a redelivery doesn't post again.
const feature = issue(7, "[Feature] Android app");
assert.equal(await hook("opened", feature), 204);
assert.equal(await hook("opened", feature), 204);
assert.deepEqual(calls, ['post features "#7 [Feature] Android app" gh-f Opened on GitHub by octo: <https://github.com/leepokai/Codync/issues/7>']);
assert.deepEqual(JSON.parse(links.get("issue:7")!), { kind: "feature", thread: "t7" });

// Closed as completed: said in the thread, Shipped added, closed; reopened: said, Shipped removed, open again.
calls = []; threadTags = ["gh-f"];
assert.equal(await hook("closed", { ...feature, state_reason: "completed" }), 204);
assert.equal(await hook("reopened", feature), 204);
assert.deepEqual(calls, [
  "say t7 ✅ Closed as completed on GitHub: <https://github.com/leepokai/Codync/issues/7>",
  "set t7 [gh-f,shipped] archived=true",
  "say t7 🔄 Reopened on GitHub: <https://github.com/leepokai/Codync/issues/7>",
  "set t7 [gh-f] archived=false",
]);
calls = [];
assert.equal(await hook("closed", { ...feature, state_reason: "not_planned" }), 204);
assert.deepEqual(calls, ["say t7 Closed as not planned on GitHub: <https://github.com/leepokai/Codync/issues/7>", "set t7 [gh-f] archived=true"]);

// Daily run: open issues without a post get one (an `enhancement` label counts as a feature); pull requests don't.
calls = [];
open = [feature, issue(8, "Crash on launch"), issue(9, "Custom icons", { labels: [{ name: "enhancement" }] }), issue(10, "A PR", { pull_request: {} })];
await worker.scheduled({} as ScheduledController, env);
assert.deepEqual(calls.map((c) => c.split(" Opened")[0]), ['post bugs "#8 Crash on launch" gh-b', 'post features "#9 Custom icons" gh-f']);
console.log("issue tests ok");
