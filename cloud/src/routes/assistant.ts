// The Codync connector for the user's cloud assistant (ChatGPT, Claude…) at mcp.codync.dev (§7.9): an MCP
// server over Streamable HTTP (JSON answers, no sessions, no server stream) for stateless clients
// (2026-07-28) and `initialize` ones alike. Clerk is its authorization server, so an assistant signs in
// with the Codync account and gets a token for this server's URL alone. Tool calls go to the account's computers over
// their relay sockets, and only a computer whose user turned cloud assistants on answers them. Unlike the
// device channel, the cloud sees these calls.

import type { Answer } from "../host-calls";
import { verifyAssistant } from "../auth";
import { ApiError, type Ctx } from "./common";

/** Newest first; the first takes no `initialize` (every request names its version). */
const PROTOCOLS = ["2026-07-28", "2025-11-25", "2025-06-18", "2025-03-26"];
const LEGACY = PROTOCOLS.slice(1);
const VERSION_KEY = "io.modelcontextprotocol/protocolVersion";
const SERVER_INFO = { name: "codync", title: "Codync", version: "2", websiteUrl: "https://codync.dev" };
/** The tool list is the same for everyone and changes only with a deploy. */
const TOOLS_TTL_MS = 3_600_000;
const SCOPES = ["profile", "email"];
const DOCS = "https://codync.dev/docs/cloud-assistants";

const INSTRUCTIONS =
  "Codync connects you to the user's own coding bots, running on their computers. Use list_bots to find one, " +
  "message_bot to give it a self-contained task, and read_replies to collect its answer. Bots work in their own " +
  "folders with their own tools and can take minutes: after messaging one, tell the user what you asked for and " +
  "check read_replies when they come back or ask, instead of waiting in a loop.";

const COMPUTER = {
  type: "string",
  description: "Which of the user's computers, by name from list_computers. Needed only when several are online.",
};

const TOOLS = [
  {
    name: "list_computers",
    title: "List computers",
    description: "The computers in the user's Codync account, and whether each is online.",
    inputSchema: { type: "object", properties: {} },
    annotations: { readOnlyHint: true, openWorldHint: false },
  },
  {
    name: "list_bots",
    title: "List bots",
    description:
      "The user's Codync bots you can message, with their IDs, kinds, specialties and status, also those on the user's " +
      "other computers in that computer's network (IDs like reviewer@laptop), where Claude Code sessions and Codex " +
      "threads can be offered too. Narrow it with query or kind; it returns at most limit at a time, with next for " +
      "the rest. Without computer, every online computer answers. Rows without kind come from a computer whose " +
      "Codync is older and doesn't filter by kind.",
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string", description: "Words that must all appear in the bot's name or description." },
        kind: {
          type: "string",
          enum: ["codync", "claude-code", "codex", "terminal", "other"],
          description:
            "Only this kind of agent: codync (Codync bots), claude-code (Claude Code sessions), codex (Codex threads), " +
            "terminal (a person at a terminal on another computer) or other (on a computer whose Codync doesn't say " +
            "its kind yet).",
        },
        limit: { type: "integer", minimum: 1, maximum: 50, description: "At most this many (default 20)." },
        cursor: { type: "string", description: "next from the previous call, for the rest." },
        computer: COMPUTER,
      },
    },
    annotations: { readOnlyHint: true, openWorldHint: false },
  },
  {
    name: "message_bot",
    title: "Message a bot",
    description:
      "Send a self-contained request to one of the user's Codync bots. It runs in that bot's own chat on the " +
      "user's computer, with the bot's tools, folder and permissions, and this returns as soon as it is queued. " +
      "Get the reply later with read_replies.",
    inputSchema: {
      type: "object",
      properties: {
        botId: {
          type: "string",
          description: "The bot's id from list_bots; with computer set to the computer that listed it when several are online.",
        },
        message: { type: "string", description: "The task, relevant context, file paths and the expected result." },
        computer: COMPUTER,
      },
      required: ["botId", "message"],
    },
    annotations: { readOnlyHint: false, idempotentHint: false, openWorldHint: false },
  },
  {
    name: "read_replies",
    title: "Read replies",
    description:
      "Your recent message_bot requests, newest first, with each bot's reply once it's done. status is queued, " +
      "sent (running), completed, failed or cancelled. Give requestId for one request.",
    inputSchema: {
      type: "object",
      properties: {
        requestId: { type: "string", description: "One request's id from message_bot." },
        computer: COMPUTER,
      },
    },
    annotations: { readOnlyHint: true, openWorldHint: false },
  },
];

const CORS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
  "Access-Control-Allow-Headers": "Authorization, Content-Type, Mcp-Protocol-Version, Mcp-Session-Id",
  "Access-Control-Expose-Headers": "WWW-Authenticate",
};

const json = (v: unknown, status = 200, headers: Record<string, string> = {}) =>
  Response.json(v, { status, headers: { ...CORS, ...headers } });

/** A tool's own failure: the assistant sees it as the tool's answer (`isError`). */
class ToolError extends Error {}

export const assistantPreflight = async () => new Response(null, { status: 204, headers: CORS });

/** Protected resource metadata (RFC 9728): Clerk issues the tokens. */
export const resourceMetadata = async (c: Ctx) =>
  json({
    resource: `${c.url.origin}/mcp`,
    authorization_servers: [c.env.CLERK_ISSUER],
    scopes_supported: SCOPES,
    bearer_methods_supported: ["header"],
    resource_name: "Codync",
    resource_documentation: DOCS,
  });

/** Clerk's own metadata, for clients that still look for it on this host. */
export async function authServerMetadata(c: Ctx) {
  const res = await fetch(`${c.env.CLERK_ISSUER}/.well-known/oauth-authorization-server`, { cf: { cacheTtl: 3600 } });
  if (!res.ok) throw new ApiError("internal", "The sign-in service didn't answer");
  return json(await res.json());
}

/** The account the request's token stands for, or `null` and the challenge that says where to sign in. */
async function signedIn(c: Ctx): Promise<[string, null] | [null, Response]> {
  const user = await verifyAssistant(c.req, c.env, `${c.url.origin}/mcp`);
  if (user) return [user, null];
  const metadata = `${c.url.origin}/.well-known/oauth-protected-resource/mcp`;
  const challenge = `Bearer resource_metadata="${metadata}", scope="${SCOPES.join(" ")}"`;
  return [null, json({ error: "invalid_token" }, 401, { "WWW-Authenticate": challenge })];
}

/** No server-to-client stream: everything is a POST with its JSON answer. */
export async function mcpStream(c: Ctx): Promise<Response> {
  const [, refused] = await signedIn(c);
  return refused ?? new Response(null, { status: 405, headers: { ...CORS, Allow: "POST" } });
}

export async function mcp(c: Ctx): Promise<Response> {
  const [user, refused] = await signedIn(c);
  if (refused) return refused;
  if (c.env.ASSISTANT_LIMITER && !(await c.env.ASSISTANT_LIMITER.limit({ key: user })).success) {
    return json({ error: "rate_limited" }, 429, { "Retry-After": "60" });
  }
  let msg: { id?: unknown; method?: unknown; params?: Record<string, unknown> };
  try {
    msg = JSON.parse(new TextDecoder().decode(c.raw));
  } catch {
    return json(failure(null, -32700, "Parse error"));
  }
  if (!msg || typeof msg !== "object" || Array.isArray(msg)) return json(failure(null, -32600, "Invalid request"));
  // A notification or a response: nothing to answer.
  if (msg.id === undefined || msg.method === undefined) return new Response(null, { status: 202, headers: CORS });
  const id = msg.id;
  const meta = msg.params?._meta as Record<string, unknown> | undefined;
  const version = meta?.[VERSION_KEY] ?? c.req.headers.get("MCP-Protocol-Version");
  if (msg.method !== "initialize" && typeof version === "string" && !PROTOCOLS.includes(version)) {
    return json(failure(id, -32022, "Unsupported protocol version", { supported: PROTOCOLS, requested: version }));
  }
  switch (msg.method) {
    case "server/discover":
      return json(
        success(id, {
          supportedVersions: PROTOCOLS,
          capabilities: { tools: {} },
          instructions: INSTRUCTIONS,
          ttlMs: TOOLS_TTL_MS,
          cacheScope: "public",
        }),
      );
    case "initialize": {
      const asked = msg.params?.protocolVersion;
      return json(
        success(id, {
          protocolVersion: typeof asked === "string" && LEGACY.includes(asked) ? asked : LEGACY[0],
          capabilities: { tools: {} },
          serverInfo: SERVER_INFO,
          instructions: INSTRUCTIONS,
        }),
      );
    }
    case "ping":
      return json(success(id, {}));
    case "tools/list":
      return json(success(id, { tools: TOOLS, ttlMs: TOOLS_TTL_MS, cacheScope: "public" }));
    case "tools/call": {
      const name = msg.params?.name;
      const args = msg.params?.arguments ?? {};
      if (typeof name !== "string" || !TOOLS.some((t) => t.name === name)) return json(failure(id, -32602, `Unknown tool: ${name}`));
      if (typeof args !== "object" || Array.isArray(args)) return json(failure(id, -32602, "arguments must be an object"));
      try {
        const result = await tool(c, user, name, args as Record<string, unknown>);
        return json(success(id, { content: [{ type: "text", text: JSON.stringify(result) }] }));
      } catch (e) {
        if (!(e instanceof ToolError)) throw e;
        return json(success(id, { content: [{ type: "text", text: e.message }], isError: true }));
      }
    }
    default:
      return json(failure(id, -32601, "Method not found"));
  }
}

/** Every result is complete (no follow-up rounds) and names this server. */
const success = (id: unknown, result: Record<string, unknown>) => ({
  jsonrpc: "2.0",
  id,
  result: { resultType: "complete", _meta: { "io.modelcontextprotocol/serverInfo": SERVER_INFO }, ...result },
});
const failure = (id: unknown, code: number, message: string, data?: unknown) => ({
  jsonrpc: "2.0",
  id,
  error: data === undefined ? { code, message } : { code, message, data },
});

interface Computer {
  id: string;
  name: string;
  online: number;
  last_seen_at: number | null;
}

async function tool(c: Ctx, user: string, name: string, args: Record<string, unknown>): Promise<unknown> {
  const { results: all } = await c.env.DB.prepare(
    "SELECT id, name, online, last_seen_at FROM computers WHERE owner_user_id = ? AND status = 'active' ORDER BY claimed_at",
  )
    .bind(user)
    .all<Computer>();
  if (name === "list_computers") {
    return { computers: all.map((x) => ({ name: x.name, online: !!x.online, lastSeenAt: x.last_seen_at })) };
  }
  if (!all.length) throw new ToolError("No computer is in this Codync account yet: sign in to Codync on a computer first.");
  // Every other tool (`mcp` takes only listed ones) is the computer's to answer.
  const { computer: wanted, ...rest } = args;
  const online = all.filter((x) => x.online);
  // Every online computer answers a listing that names none.
  if (wanted === undefined && name !== "message_bot" && online.length > 1) {
    return {
      computers: await Promise.all(
        online.map((x) =>
          ask(c, x, user, name, rest).then(
            (result) => ({ computer: x.name, ...(result as object) }),
            (e: Error) => ({ computer: x.name, error: e.message }),
          ),
        ),
      ),
    };
  }
  return ask(c, pick(all, online, wanted), user, name, rest);
}

function pick(all: Computer[], online: Computer[], wanted: unknown): Computer {
  const names = (list: Computer[]) => list.map((x) => x.name).join(", ");
  if (typeof wanted === "string" && wanted) {
    const hit = all.find((x) => x.name.toLowerCase() === wanted.toLowerCase() || x.id === wanted);
    if (!hit) throw new ToolError(`No computer called ${wanted} in this account: ${names(all)}.`);
    return hit;
  }
  if (all.length === 1) return all[0]!;
  if (online.length === 1) return online[0]!;
  throw new ToolError(
    online.length ? `Say which computer: ${names(online)}.` : `None of the user's computers is online: ${names(all)}.`,
  );
}

async function ask(c: Ctx, computer: Computer, user: string, tool: string, args: Record<string, unknown>): Promise<unknown> {
  const res = await c.env.RELAY.get(c.env.RELAY.idFromName(computer.id)).fetch(
    new Request("https://do/internal/assistant", {
      method: "POST",
      headers: { "X-Codync-Internal": "1" },
      body: JSON.stringify({ user, tool, args }),
    }),
  );
  const a = (await res.json()) as Answer;
  if (a.ok) return a.result;
  if (a.code === "offline") throw new ToolError(`${computer.name} is offline: Codync isn't running there, or it can't reach the internet.`);
  if (a.code === "timeout") {
    throw new ToolError(
      `${computer.name} didn't answer. Its Codync may be too old for cloud assistants: update it, then turn them on there (Cloud assistants in its Codync settings, or codync-host cloud --assistants on).`,
    );
  }
  throw new ToolError(a.error ?? "Refused");
}
