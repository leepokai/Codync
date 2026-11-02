// The Codync server: adding members, the GitHub roles they earn, and posting to its channels.

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

export const bot = (env: Env) => ({ Authorization: `Bot ${env.DISCORD_BOT_TOKEN}` });
export const github = (env: Env) => ({ headers: { Authorization: `Bearer ${env.GITHUB_TOKEN}`, "User-Agent": "codync-discord" } });

/** Every item of a paged GitHub list under the repo (`path` is relative to /repos/<repo>/). */
export async function githubAll<T>(env: Env, path: string): Promise<T[]> {
  const all: T[] = [];
  for (let page = 1; ; page++) {
    const items = await json<T[]>(`https://api.github.com/repos/${env.REPO}/${path}${path.includes("?") ? "&" : "?"}per_page=100&page=${page}`, github(env));
    all.push(...items);
    if (items.length < 100) return all;
  }
}

/** The account's current login; names in connections go stale after a rename, ids don't. */
export async function githubLogin(env: Env, githubId: string): Promise<string> {
  return (await json<{ login: string }>(`https://api.github.com/user/${githubId}`, github(env))).login;
}

/** The GitHub user id behind a login, or null when there's no such account. */
export async function githubId(env: Env, login: string): Promise<string | null> {
  const res = await fetch(`https://api.github.com/users/${login}`, github(env));
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(`user ${login} ${res.status}: ${await res.text()}`);
  return String((await res.json<{ id: number }>()).id);
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

/**
 * Posts to a channel and says whether it went out. A failure is logged and never fails the caller's flow.
 * Mentions in it notify no one.
 */
export async function send(env: Env, channelId: string, content: string): Promise<boolean> {
  const res = await fetch(`${DISCORD}/channels/${channelId}/messages`, {
    method: "POST",
    headers: { ...bot(env), "Content-Type": "application/json" },
    // Flag 4 drops link previews.
    body: JSON.stringify({ content, allowed_mentions: { parse: [] }, flags: 4 }),
  });
  if (!res.ok) console.error("send failed", { channel: channelId, status: res.status, body: await res.text() });
  return res.ok;
}

/** Posts to the staff channel. */
export async function notify(env: Env, content: string): Promise<void> {
  await send(env, env.STAFF_CHANNEL_ID, content);
}
