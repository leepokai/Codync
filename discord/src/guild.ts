// The Codync server: adding members, the GitHub roles they earn, and the staff channel notice.

import { json } from "./http.ts";
import type { Env } from "./index.ts";

export const DISCORD = "https://discord.com/api/v10";

export interface Role {
  name: string;
  id: string;
  query: string;
}

export const roles = (env: Env): Role[] => [
  { name: "Contributor", id: env.CONTRIBUTOR_ROLE_ID, query: "is:pr is:merged" },
  { name: "Reporter", id: env.REPORTER_ROLE_ID, query: "is:issue" },
];

export const names = (list: Role[]) => list.map((r) => r.name).join(" and ");
export const profile = (login: string) => `[${login}](https://github.com/${login})`;

const bot = (env: Env) => ({ Authorization: `Bot ${env.DISCORD_BOT_TOKEN}` });
const github = (env: Env) => ({ headers: { Authorization: `Bearer ${env.GITHUB_TOKEN}`, "User-Agent": "codync-discord" } });

/** The account's current login; names in connections go stale after a rename, ids don't. */
export async function githubLogin(env: Env, githubId: string): Promise<string> {
  return (await json<{ login: string }>(`https://api.github.com/user/${githubId}`, github(env))).login;
}

/** Roles the GitHub account has earned in the repo so far. */
export async function earnedRoles(env: Env, login: string): Promise<Role[]> {
  const all = roles(env);
  const counts = await Promise.all(all.map(async ({ query }) => {
    const q = encodeURIComponent(`repo:${env.REPO} ${query} author:${login}`);
    return (await json<{ total_count: number }>(`https://api.github.com/search/issues?q=${q}&per_page=1`, github(env))).total_count;
  }));
  return all.filter((_, i) => counts[i] > 0);
}

/** Adds the member to the server with their `guilds.join` token; a no-op when they're already in. */
export async function join(env: Env, discordId: string, accessToken: string): Promise<void> {
  const res = await fetch(`${DISCORD}/guilds/${env.GUILD_ID}/members/${discordId}`, {
    method: "PUT",
    headers: { ...bot(env), "Content-Type": "application/json" },
    body: JSON.stringify({ access_token: accessToken }),
  });
  if (!res.ok) throw new Error(`join ${res.status}: ${await res.text()}`);
}

/** Gives the earned roles the member doesn't hold yet and returns them; none once they've left the server. */
export async function grant(env: Env, discordId: string, earned: Role[], reason: string): Promise<Role[]> {
  if (earned.length === 0) return [];
  const member = `${DISCORD}/guilds/${env.GUILD_ID}/members/${discordId}`;
  const res = await fetch(member, { headers: bot(env) });
  if (res.status === 404) return [];
  if (!res.ok) throw new Error(`member ${res.status}: ${await res.text()}`);
  const held = (await res.json<{ roles: string[] }>()).roles;
  const missing = earned.filter((r) => !held.includes(r.id));
  for (const role of missing) {
    const put = await fetch(`${member}/roles/${role.id}`, { method: "PUT", headers: { ...bot(env), "X-Audit-Log-Reason": reason } });
    if (!put.ok) throw new Error(`role ${role.name} ${put.status}: ${await put.text()}`);
  }
  return missing;
}

/** Posts to the staff channel. A failure is logged and never fails the member's flow. */
export async function notify(env: Env, content: string): Promise<void> {
  const res = await fetch(`${DISCORD}/channels/${env.STAFF_CHANNEL_ID}/messages`, {
    method: "POST",
    headers: { ...bot(env), "Content-Type": "application/json" },
    // No pings; flag 4 drops link previews.
    body: JSON.stringify({ content, allowed_mentions: { parse: [] }, flags: 4 }),
  });
  if (!res.ok) console.error("notify failed", { status: res.status, body: await res.text() });
}
