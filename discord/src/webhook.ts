// GitHub webhook (pull_request, issues): a linked member earns a role as soon as their pull request merges or
// their issue opens, without opening the link again.

import { grant, notify, profile, roles, type Role } from "./guild.ts";
import { verify } from "./hmac.ts";
import type { Env } from "./index.ts";

interface Item {
  number: number;
  html_url: string;
  user: { id: number; login: string };
}

interface Payload {
  action: string;
  pull_request?: Item & { merged: boolean };
  issue?: Item;
}

const done = () => new Response(null, { status: 204 });

export async function webhook(req: Request, env: Env): Promise<Response> {
  const body = await req.text();
  const signature = req.headers.get("X-Hub-Signature-256")?.replace(/^sha256=/, "") ?? "";
  if (req.method !== "POST" || !(await verify(env.GITHUB_WEBHOOK_SECRET, body, signature))) {
    return new Response("Bad signature", { status: 401 });
  }
  const earned = earnedBy(req.headers.get("X-GitHub-Event"), JSON.parse(body) as Payload, env);
  if (!earned) return done();
  const { role, item, what } = earned;
  const discordId = await env.LINKS.get(`github:${item.user.id}`);
  if (!discordId) return done();

  const granted = await grant(env, discordId, [role], `GitHub @${item.user.login} ${what}`);
  if (granted.length > 0) {
    await notify(env, `<@${discordId}> (${profile(item.user.login)}) earned ${role.name} with [${what}](${item.html_url})`);
    console.log("role earned", { discord: discordId, github: item.user.login, role: role.name });
  }
  return done();
}

function earnedBy(event: string | null, payload: Payload, env: Env): { role: Role; item: Item; what: string } | null {
  const [contributor, reporter] = roles(env);
  const { pull_request: pr, issue } = payload;
  if (event === "pull_request" && payload.action === "closed" && pr?.merged) {
    return { role: contributor, item: pr, what: `PR #${pr.number}` };
  }
  if (event === "issues" && payload.action === "opened" && issue) {
    return { role: reporter, item: issue, what: `issue #${issue.number}` };
  }
  return null;
}
