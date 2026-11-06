// The cloud assistant's MCP connector (§7.9): OAuth discovery, token checks, MCP methods and tool calls
// that reach the account's computers over their relay sockets.

import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { HostCalls } from "../src/host-calls";
import { clerkToken, env, hostSocket, newHost, ORIGIN, type Json, type Sock, type TestHost } from "./helpers";

const oauthToken = (sub: string, aud = `${ORIGIN}/mcp`) =>
  clerkToken(sub, { aud, client_id: "https://assistant.example/client.json", scope: "profile email", jti: "oat_test" }, "at+jwt");

let next = 0;
async function rpc(token: string | null, method: string, params?: unknown) {
  const res = await SELF.fetch(`${ORIGIN}/mcp`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Accept: "application/json, text/event-stream", ...(token ? { Authorization: `Bearer ${token}` } : {}) },
    body: JSON.stringify({ jsonrpc: "2.0", id: ++next, method, params }),
  });
  return { status: res.status, headers: res.headers, body: res.status === 200 ? ((await res.json()) as Json) : null };
}

/** A tool call's answer: the parsed result, or the error text the assistant sees. */
async function callTool(token: string, name: string, args: Record<string, unknown> = {}) {
  const r = await rpc(token, "tools/call", { name, arguments: args });
  const { content, isError } = r.body.result;
  return isError ? { error: content[0].text as string } : { result: JSON.parse(content[0].text) as Json };
}

/** A registered computer owned by `userId`, named `name`. */
async function owned(userId: string, name: string): Promise<TestHost> {
  const host = await newHost();
  await env.DB.batch([
    env.DB.prepare("INSERT OR IGNORE INTO accounts(user_id, email, created_at) VALUES (?, NULL, ?)").bind(userId, Date.now()),
    env.DB.prepare("UPDATE computers SET owner_user_id = ?, name = ?, claimed_at = ? WHERE id = ?").bind(userId, name, Date.now(), host.cid),
  ]);
  return host;
}

/** The host answers the next cloud assistant call with `answer`; returns the call it got. */
async function answers(ws: Sock, answer: Record<string, unknown>): Promise<Json> {
  const call = await ws.nextT("assistant.call");
  ws.send({ t: "assistant.result", id: call.id, ...answer });
  return call;
}

describe("discovery and sign-in", () => {
  it("points assistants at Clerk to sign in", async () => {
    for (const path of ["/.well-known/oauth-protected-resource", "/.well-known/oauth-protected-resource/mcp"]) {
      const meta = (await (await SELF.fetch(ORIGIN + path)).json()) as Json;
      expect(meta.resource).toBe(`${ORIGIN}/mcp`);
      expect(meta.authorization_servers).toEqual([env.CLERK_ISSUER]);
    }
    const preflight = await SELF.fetch(`${ORIGIN}/mcp`, { method: "OPTIONS" });
    expect(preflight.status).toBe(204);
    expect(preflight.headers.get("Access-Control-Allow-Headers")).toContain("Authorization");
  });

  it("refuses a missing token, an app session token and one for another server, with the metadata's address", async () => {
    for (const token of [null, await clerkToken("user_session"), await oauthToken("user_session", "https://elsewhere.example/mcp")]) {
      const r = await rpc(token, "tools/list");
      expect(r.status).toBe(401);
      expect(r.headers.get("WWW-Authenticate")).toBe(
        `Bearer resource_metadata="${ORIGIN}/.well-known/oauth-protected-resource/mcp", scope="profile email"`,
      );
    }
  });

  it("serves stateless clients", async () => {
    const token = await oauthToken("user_modern");
    const meta = { _meta: { "io.modelcontextprotocol/protocolVersion": "2026-07-28" } };
    const found = (await rpc(token, "server/discover", meta)).body.result;
    expect(found).toMatchObject({ resultType: "complete", supportedVersions: expect.arrayContaining(["2026-07-28"]), capabilities: { tools: {} } });
    expect(found._meta["io.modelcontextprotocol/serverInfo"].name).toBe("codync");
    const listed = (await rpc(token, "tools/list", meta)).body.result;
    expect(listed).toMatchObject({ resultType: "complete", cacheScope: "public", ttlMs: expect.any(Number) });
    const old = await rpc(token, "tools/list", { _meta: { "io.modelcontextprotocol/protocolVersion": "1900-01-01" } });
    expect(old.body.error).toMatchObject({ code: -32022, data: { requested: "1900-01-01", supported: expect.arrayContaining(["2026-07-28"]) } });
  });

  it("answers initialize, ping and tools/list, and takes notifications", async () => {
    const token = await oauthToken("user_init");
    const init = await rpc(token, "initialize", { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "t", version: "1" } });
    expect(init.body.result.protocolVersion).toBe("2025-06-18");
    expect(init.body.result.capabilities).toEqual({ tools: {} });
    expect((await rpc(token, "initialize", { protocolVersion: "1999-01-01" })).body.result.protocolVersion).toBe("2025-11-25");
    expect((await rpc(token, "ping")).body.result.resultType).toBe("complete");
    const tools = (await rpc(token, "tools/list")).body.result.tools.map((t: Json) => t.name);
    expect(tools).toEqual(["list_computers", "list_bots", "message_bot", "read_replies"]);
    const note = await SELF.fetch(`${ORIGIN}/mcp`, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }),
    });
    expect(note.status).toBe(202);
    expect((await rpc(token, "tools/call", { name: "rm_rf" })).body.error.code).toBe(-32602);
    expect((await rpc(token, "resources/list")).body.error.code).toBe(-32601);
    expect((await SELF.fetch(`${ORIGIN}/mcp`)).status).toBe(401);
    expect((await SELF.fetch(`${ORIGIN}/mcp`, { headers: { Authorization: `Bearer ${token}` } })).status).toBe(405);
  });
});

describe("tool calls", () => {
  it("reach the account's computer and pass its answer or refusal back", async () => {
    const token = await oauthToken("user_one");
    expect((await callTool(token, "list_bots")).error).toContain("No computer is in this Codync account");
    const host = await owned("user_one", "desk");
    expect((await callTool(token, "list_computers")).result.computers).toEqual([{ name: "desk", online: false, lastSeenAt: null }]);
    expect((await callTool(token, "list_bots")).error).toBe("desk is offline: Codync isn't running there, or it can't reach the internet.");

    const ws = await hostSocket(host);
    const listing = callTool(token, "list_bots", { query: "review" });
    const call = await answers(ws, { ok: true, result: { bots: [{ id: "b1", name: "Reviewer" }], total: 1 } });
    expect(call).toMatchObject({ t: "assistant.call", user: "user_one", tool: "list_bots", args: { query: "review" } });
    expect((await listing).result).toEqual({ bots: [{ id: "b1", name: "Reviewer" }], total: 1 });

    const refused = callTool(token, "message_bot", { botId: "b1", message: "hi" });
    await answers(ws, { ok: false, error: "Cloud assistants are off on this computer." });
    expect((await refused).error).toBe("Cloud assistants are off on this computer.");

    // Another account's token never reaches this computer.
    expect((await callTool(await oauthToken("user_other"), "list_bots")).error).toContain("No computer");
    ws.ws.close();
  });

  it("asks every online computer for a listing, and which one for a message", async () => {
    const token = await oauthToken("user_two");
    const desk = await hostSocket(await owned("user_two", "desk"));
    const laptop = await hostSocket(await owned("user_two", "laptop"));
    await owned("user_two", "server");
    await expect.poll(async () => (await callTool(token, "list_computers")).result.computers.filter((c: Json) => c.online).length).toBe(2);

    const listing = callTool(token, "list_bots");
    await answers(desk, { ok: true, result: { bots: [{ id: "d" }] } });
    await answers(laptop, { ok: false, error: "Cloud assistants are off on this computer." });
    expect((await listing).result.computers).toEqual(
      expect.arrayContaining([
        { computer: "desk", bots: [{ id: "d" }] },
        { computer: "laptop", error: "Cloud assistants are off on this computer." },
      ]),
    );
    expect((await callTool(token, "message_bot", { botId: "d", message: "hi" })).error).toBe("Say which computer: desk, laptop.");
    expect((await callTool(token, "message_bot", { botId: "d", message: "hi", computer: "nas" })).error).toBe(
      "No computer called nas in this account: desk, laptop, server.",
    );
    const sent = callTool(token, "message_bot", { botId: "d", message: "hi", computer: "Laptop" });
    const call = await answers(laptop, { ok: true, result: { requestId: "r1" } });
    expect(call.args).toEqual({ botId: "d", message: "hi" });
    expect((await sent).result).toEqual({ requestId: "r1" });
    desk.ws.close();
    laptop.ws.close();
  });

  it("ends a call whose host goes away", async () => {
    const token = await oauthToken("user_gone");
    const ws = await hostSocket(await owned("user_gone", "desk"));
    const listing = callTool(token, "list_bots");
    await ws.nextT("assistant.call");
    ws.ws.close();
    expect((await listing).error).toContain("desk is offline");
  });
});

describe("host calls", () => {
  it("time out when the host never answers (an older host ignores them)", async () => {
    const sent: string[] = [];
    const calls = new HostCalls();
    const answer = await calls.ask({ send: (m: string) => sent.push(m) } as unknown as WebSocket, { t: "assistant.call" }, 20);
    expect(answer).toEqual({ ok: false, code: "timeout" });
    expect(JSON.parse(sent[0]!)).toMatchObject({ t: "assistant.call", id: expect.any(String) });
  });
});
