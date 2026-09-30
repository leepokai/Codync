import { afterEach, describe, expect, it, vi } from "vitest";
import worker, { type Env } from "../src/index";
import { AUTHORITY, ORIGIN, env, newHost, type TestHost } from "./helpers";
import * as ref from "./ref";

const path = "/v1/host/screen-ice";
const deviceKey = ref.b64url(ref.signKey().pub);
const servers = [
  { urls: ["stun:stun.cloudflare.com:3478"] },
  { urls: ["turn:turn.cloudflare.com:3478?transport=udp", "turns:turn.cloudflare.com:443?transport=tcp"], username: "short-user", credential: "short-secret" },
];

async function claimedHost() {
  const host = await newHost();
  const userId = `turn_${crypto.randomUUID()}`;
  await env.DB.prepare("INSERT INTO accounts(user_id, created_at) VALUES (?, ?)").bind(userId, Date.now()).run();
  await env.DB.prepare("UPDATE computers SET owner_user_id = ? WHERE id = ?").bind(userId, host.cid).run();
  return { host, userId };
}

function request(host: TestHost, overrides: Partial<Env> = {}, body = { deviceKey }) {
  const bytes = ref.enc.encode(JSON.stringify(body));
  const sig = ref.signRequest(host.sign, { method: "POST", authority: AUTHORITY, pathAndQuery: path, body: bytes });
  return worker.fetch(new Request(ORIGIN + path, {
    method: "POST", headers: { "Codync-Sig": sig }, body: bytes,
  }), { ...env, TURN_KEY_ID: "test-key", TURN_KEY_API_TOKEN: "server-secret", ...overrides }, {} as ExecutionContext);
}

afterEach(() => vi.restoreAllMocks());

describe("screen relay credentials", () => {
  it("issues expiring credentials for a claimed host and tags device usage", async () => {
    const { host } = await claimedHost();
    const fetch = vi.spyOn(globalThis, "fetch").mockImplementation(async (_url, init) => {
      const payload = JSON.parse(init!.body as string);
      // Match the live provider, which rejects the original 66-character tag.
      if (payload.customIdentifier.length > 64) return new Response("invalid argument", { status: 400 });
      return Response.json({ iceServers: servers });
    });
    const start = Date.now();
    const res = await request(host);
    expect(res.status).toBe(200);
    expect(res.headers.get("Cache-Control")).toBe("no-store");
    const data = await res.json() as { iceServers: unknown; expiresAt: number };
    expect(data.iceServers).toEqual(servers);
    expect(data.expiresAt).toBeGreaterThanOrEqual(start + 3_600_000);
    expect(data.expiresAt).toBeLessThanOrEqual(Date.now() + 3_600_000);
    const [url, init] = fetch.mock.calls[0]!;
    expect(url).toBe("https://rtc.live.cloudflare.com/v1/turn/keys/test-key/credentials/generate-ice-servers");
    expect(JSON.parse(init!.body as string)).toEqual({ ttl: 3600, customIdentifier: `${host.cid}:${ref.computerId(ref.unb64url(deviceKey))}` });
    expect(JSON.stringify(data)).not.toContain("server-secret");
    const repeated = await request(host);
    expect(repeated.status).toBe(200);
    expect(JSON.parse(fetch.mock.calls[1]![1]!.body as string).customIdentifier)
      .toBe(JSON.parse(init!.body as string).customIdentifier);
    const otherDevice = ref.b64url(ref.signKey().pub);
    expect((await request(host, {}, { deviceKey: otherDevice })).status).toBe(200);
    expect(JSON.parse(fetch.mock.calls[2]![1]!.body as string).customIdentifier)
      .not.toBe(JSON.parse(init!.body as string).customIdentifier);
  });

  it("rejects anonymous registration, blocked computers and deleted accounts before contacting TURN", async () => {
    const fetch = vi.spyOn(globalThis, "fetch");
    const unclaimed = await newHost();
    expect((await request(unclaimed)).status).toBe(403);
    const { host, userId } = await claimedHost();
    await env.DB.prepare("UPDATE accounts SET status = 'deleted' WHERE user_id = ?").bind(userId).run();
    expect((await request(host)).status).toBe(403);
    await env.DB.prepare("UPDATE computers SET status = 'blocked' WHERE id = ?").bind(host.cid).run();
    expect((await request(host)).status).toBe(403);
    expect(fetch).not.toHaveBeenCalled();
  });

  it("requires host signature and a device identity", async () => {
    const res = await worker.fetch(new Request(ORIGIN + path, { method: "POST" }), env, {} as ExecutionContext);
    expect(res.status).toBe(401);
    const { host } = await claimedHost();
    expect((await request(host, {}, { deviceKey: "invalid" })).status).toBe(400);
  });

  it("reports missing setup and enforces the per-account issuance limit", async () => {
    const { host } = await claimedHost();
    expect((await request(host, { TURN_KEY_API_TOKEN: "" })).status).toBe(503);
    const fetch = vi.spyOn(globalThis, "fetch").mockImplementation(async () => Response.json({ iceServers: servers }));
    for (let i = 0; i < 12; i++) expect((await request(host)).status).toBe(200);
    expect((await request(host)).status).toBe(429);
    expect(fetch).toHaveBeenCalledTimes(12);
  });

  it.each([
    { iceServers: [] },
    { iceServers: [{ urls: ["turn:evil.example:3478"], username: "u", credential: "p" }] },
    { iceServers: [{ urls: ["turn:turn.cloudflare.com:3478"], username: "u" }] },
    { iceServers: [{ urls: ["stun:stun.cloudflare.com:3478"] }] },
  ])("rejects malformed provider responses", async (data) => {
    const { host } = await claimedHost();
    vi.spyOn(globalThis, "fetch").mockResolvedValue(Response.json(data));
    expect((await request(host)).status).toBe(503);
  });

  it("does not expose provider error bodies or secrets", async () => {
    const { host } = await claimedHost();
    vi.spyOn(globalThis, "fetch").mockResolvedValue(new Response("server-secret", { status: 401 }));
    const res = await request(host);
    expect(res.status).toBe(503);
    expect(await res.text()).not.toContain("server-secret");
  });
});
