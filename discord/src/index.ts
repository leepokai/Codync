// GitHub roles for the Codync Discord, from one link. Signing in with Discord adds the member to the server;
// their GitHub account comes from their Discord connections (verified by Discord) or, when they have none, from
// signing in with GitHub, so nobody can claim someone else's. A merged pull request on the repo earns the
// Contributor role, an opened issue the Reporter role. The link is remembered, so later ones arrive on their
// own (webhook.ts).

import { DISCORD, earnedRoles, githubLogin, grant, join, names, notify, profile } from "./guild.ts";
import { sign, verify } from "./hmac.ts";
import { json, page } from "./http.ts";
import { webhook } from "./webhook.ts";

export interface Env {
  DISCORD_CLIENT_ID: string;
  DISCORD_CLIENT_SECRET: string;
  DISCORD_BOT_TOKEN: string;
  GITHUB_CLIENT_ID: string;
  GITHUB_CLIENT_SECRET: string;
  GITHUB_TOKEN: string;
  GITHUB_WEBHOOK_SECRET: string;
  SESSION_SECRET: string;
  GUILD_ID: string;
  CONTRIBUTOR_ROLE_ID: string;
  REPORTER_ROLE_ID: string;
  STAFF_CHANNEL_ID: string;
  REPO: string;
  /** `github:<GitHub user id>` → Discord user id. */
  LINKS: KVNamespace;
}

export default {
  async fetch(req: Request, env: Env): Promise<Response> {
    const url = new URL(req.url);
    try {
      switch (url.pathname) {
        case "/": return discordSignIn(env, url.origin);
        case "/callback": return await discordCallback(req, url, env);
        case "/github/callback": return await githubCallback(req, url, env);
        case "/webhook": return await webhook(req, env);
        default: return page(404, "Not found", "");
      }
    } catch (e) {
      console.error("request failed", { path: url.pathname, error: String(e) });
      return page(502, "Something went wrong", "Try again in a minute.");
    }
  },
};

const expired = () => page(400, "This link expired", "Open discord.codync.dev again.");

const cookie = (req: Request, name: string) =>
  req.headers.get("Cookie")?.match(new RegExp(`(?:^|;\\s*)${name}=([^;]+)`))?.[1];

function redirect(location: string, name: string, value: string, path: string): Response {
  return new Response(null, {
    status: 302,
    headers: { Location: location, "Set-Cookie": `${name}=${value}; Path=${path}; Max-Age=600; HttpOnly; Secure; SameSite=Lax` },
  });
}

function discordSignIn(env: Env, origin: string): Response {
  const state = crypto.randomUUID();
  const params = new URLSearchParams({
    client_id: env.DISCORD_CLIENT_ID,
    response_type: "code",
    scope: "identify connections guilds.join",
    redirect_uri: `${origin}/callback`,
    state,
  });
  return redirect(`https://discord.com/oauth2/authorize?${params}`, "state", state, "/callback");
}

async function discordCallback(req: Request, url: URL, env: Env): Promise<Response> {
  const code = url.searchParams.get("code");
  const state = url.searchParams.get("state");
  if (!code || !state || state !== cookie(req, "state")) return expired();

  const { access_token } = await json<{ access_token: string }>(`${DISCORD}/oauth2/token`, {
    method: "POST",
    body: new URLSearchParams({
      client_id: env.DISCORD_CLIENT_ID,
      client_secret: env.DISCORD_CLIENT_SECRET,
      grant_type: "authorization_code",
      code,
      redirect_uri: `${url.origin}/callback`,
    }),
  });
  const user = { headers: { Authorization: `Bearer ${access_token}` } };
  const [me, connections] = await Promise.all([
    json<{ id: string }>(`${DISCORD}/users/@me`, user),
    json<{ type: string; id: string; verified: boolean }[]>(`${DISCORD}/users/@me/connections`, user),
  ]);
  await join(env, me.id, access_token);
  // The connection's id is the GitHub user id.
  const connection = connections.find((c) => c.type === "github" && c.verified);
  return connection ? link(env, me.id, connection.id) : githubSignIn(env, url.origin, me.id);
}

async function githubSignIn(env: Env, origin: string, discordId: string): Promise<Response> {
  const state = crypto.randomUUID();
  const params = new URLSearchParams({ client_id: env.GITHUB_CLIENT_ID, redirect_uri: `${origin}/github/callback`, state });
  // Signed, so a forged cookie can't link a GitHub account to someone else's Discord.
  const session = `${state}.${discordId}`;
  const value = `${session}.${await sign(env.SESSION_SECRET, session)}`;
  return redirect(`https://github.com/login/oauth/authorize?${params}`, "session", value, "/github/callback");
}

async function githubCallback(req: Request, url: URL, env: Env): Promise<Response> {
  const code = url.searchParams.get("code");
  const [state, discordId, signature] = cookie(req, "session")?.split(".") ?? [];
  if (!code || !state || !discordId || !signature || url.searchParams.get("state") !== state) return expired();
  if (!(await verify(env.SESSION_SECRET, `${state}.${discordId}`, signature))) return expired();

  const token = await json<{ access_token?: string }>("https://github.com/login/oauth/access_token", {
    method: "POST",
    headers: { Accept: "application/json" },
    body: new URLSearchParams({
      client_id: env.GITHUB_CLIENT_ID,
      client_secret: env.GITHUB_CLIENT_SECRET,
      code,
      redirect_uri: `${url.origin}/github/callback`,
    }),
  });
  // GitHub answers a used or expired code with 200 and an error field.
  if (!token.access_token) return expired();
  const github = { headers: { Authorization: `Bearer ${token.access_token}`, "User-Agent": "codync-discord" } };
  const { id } = await json<{ id: number }>("https://api.github.com/user", github);
  return link(env, discordId, String(id));
}

async function link(env: Env, discordId: string, githubId: string): Promise<Response> {
  const login = await githubLogin(env, githubId);
  const earned = await earnedRoles(env, login);
  const key = `github:${githubId}`;
  const known = (await env.LINKS.get(key)) === discordId;
  await env.LINKS.put(key, discordId);
  const granted = await grant(env, discordId, earned, `GitHub @${login}`);
  if (!known || granted.length > 0) {
    const roles = granted.length > 0 ? ` → ${names(granted)}` : "";
    await notify(env, `<@${discordId}> linked GitHub ${profile(login)}${roles}`);
  }
  console.log("linked", { discord: discordId, github: login, roles: names(earned) });

  const server = { href: `https://discord.com/channels/${env.GUILD_ID}`, label: "Open Discord" };
  if (earned.length === 0) {
    return page(200, `You're in, @${login}`, "Your roles show up on their own once you open an issue or a pull request of yours is merged.", server);
  }
  return page(200, `You're a ${names(earned)}`, `Thanks for your work on Codync, @${login}.`, server);
}
