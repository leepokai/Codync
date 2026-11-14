// GitHub issues in the Discord forums: each issue gets its own post, tagged GitHub, in #feature-requests (the
// `enhancement` label or a "[Feature]" title) or #bug-reports (everything else). Closing and reopening on GitHub
// is posted in that thread and closes or reopens it; replies in Discord stay in Discord.

import { DISCORD, bot, githubAll, send } from "./guild.ts";
import { json } from "./http.ts";
import type { Env } from "./index.ts";

export interface Issue {
  number: number;
  title: string;
  body: string | null;
  html_url: string;
  state_reason?: string | null;
  labels: { name: string }[];
  user: { login: string };
  pull_request?: unknown;
}

type Kind = "bug" | "feature";

/** Each forum and the tag a post gets when its issue is closed as completed. */
const forum = (env: Env, kind: Kind) =>
  kind === "feature" ? { id: env.FEATURE_FORUM_ID, done: "Shipped" } : { id: env.BUG_FORUM_ID, done: "Fixed" };

const kindOf = (issue: Issue): Kind =>
  /^\s*\[feature/i.test(issue.title) || issue.labels.some((l) => l.name === "enhancement") ? "feature" : "bug";

const key = (issue: Issue) => `issue:${issue.number}`;
const EXCERPT = 1200;

export async function syncIssue(env: Env, action: string, issue: Issue): Promise<void> {
  if (action === "opened") await post(env, issue);
  if (action === "closed" || action === "reopened") await setState(env, issue, action === "closed");
}

/** Posts every open issue that has no post yet: the first run brings the existing ones over, later runs catch a
 * missed webhook. */
export async function postOpenIssues(env: Env): Promise<void> {
  const open = await githubAll<Issue>(env, `issues?state=open&sort=created&direction=asc`);
  for (const issue of open.filter((i) => !i.pull_request)) await post(env, issue);
}

async function post(env: Env, issue: Issue): Promise<void> {
  if (await env.LINKS.get(key(issue))) return;
  const kind = kindOf(issue);
  const { id } = forum(env, kind);
  const tags = await tagIds(env, id);
  const thread = await json<{ id: string }>(`${DISCORD}/channels/${id}/threads`, {
    method: "POST",
    headers: { ...bot(env), "Content-Type": "application/json" },
    body: JSON.stringify({
      name: `#${issue.number} ${issue.title}`.slice(0, 100),
      applied_tags: [tags.get("GitHub")],
      message: { content: opening(issue), flags: 4, allowed_mentions: { parse: [] } },
    }),
  });
  await env.LINKS.put(key(issue), JSON.stringify({ kind, thread: thread.id }));
  console.log("issue posted", { issue: issue.number, thread: thread.id });
}

async function setState(env: Env, issue: Issue, closed: boolean): Promise<void> {
  const saved = await env.LINKS.get(key(issue));
  if (!saved) return closed ? undefined : post(env, issue);
  const { kind, thread } = JSON.parse(saved) as { kind: Kind; thread: string };
  const how = !closed ? "🔄 Reopened" : issue.state_reason === "not_planned" ? "Closed as not planned" : "✅ Closed as completed";
  // A message in a closed thread reopens it, so it goes first.
  await send(env, thread, `${how} on GitHub: <${issue.html_url}>`);
  const done = (await tagIds(env, forum(env, kind).id)).get(forum(env, kind).done);
  const current = (await json<{ applied_tags: string[] }>(`${DISCORD}/channels/${thread}`, { headers: bot(env) })).applied_tags;
  const completed = closed && issue.state_reason !== "not_planned";
  const tags = current.filter((t) => t !== done).concat(completed && done ? [done] : []);
  await json(`${DISCORD}/channels/${thread}`, {
    method: "PATCH",
    headers: { ...bot(env), "Content-Type": "application/json" },
    body: JSON.stringify({ applied_tags: tags, archived: closed }),
  });
}

/** The forum's tags by name. */
async function tagIds(env: Env, forumId: string): Promise<Map<string, string>> {
  const { available_tags } = await json<{ available_tags: { id: string; name: string }[] }>(`${DISCORD}/channels/${forumId}`, { headers: bot(env) });
  return new Map(available_tags.map((t) => [t.name, t.id]));
}

function opening(issue: Issue): string {
  const body = (issue.body ?? "").replace(/<!--[\s\S]*?-->/g, "").trim();
  const excerpt = body.length > EXCERPT ? `${body.slice(0, EXCERPT).trimEnd()}…` : body;
  return [
    // A plain login: the forum list previews this line without rendering links.
    `Opened on GitHub by ${issue.user.login}: <${issue.html_url}>`,
    excerpt,
    "-# Replies here stay in Discord. Comment on GitHub to reach the issue.",
  ].filter(Boolean).join("\n\n");
}
