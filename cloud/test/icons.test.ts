import { SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import { env, newHost, ORIGIN, AUTHORITY, type TestHost } from "./helpers";
import { signRequest } from "./ref";

async function claimedHost() {
  const h = await newHost();
  const owner = `user_${h.cid}`;
  await env.DB.prepare("INSERT INTO accounts(user_id, created_at) VALUES (?, 0)").bind(owner).run();
  await env.DB.prepare("UPDATE computers SET owner_user_id = ? WHERE id = ?").bind(owner, h.cid).run();
  return h;
}

function request(h: TestHost, method: string, slot: string, bytes?: Uint8Array) {
  const path = `/v1/host/icons/${slot}`;
  return SELF.fetch(ORIGIN + path, {
    method,
    headers: { "Codync-Sig": signRequest(h.sign, { method, authority: AUTHORITY, pathAndQuery: path, body: bytes }) },
    body: bytes,
  });
}

describe("private icon storage", () => {
  it("round trips encrypted bytes, replaces the slot, and removes it idempotently", async () => {
    const h = await claimedHost();
    const bytes = new Uint8Array(100).fill(1);
    expect((await request(h, "PUT", "0", bytes)).status).toBe(204);
    const read = await request(h, "GET", "0");
    expect(read.status).toBe(200);
    expect(read.headers.get("Cache-Control")).toBe("no-store");
    expect(new Uint8Array(await read.arrayBuffer())).toEqual(bytes);
    bytes[50] = 9;
    expect((await request(h, "PUT", "0", bytes)).status).toBe(204);
    expect(new Uint8Array(await (await request(h, "GET", "0")).arrayBuffer())).toEqual(bytes);
    expect((await request(h, "DELETE", "0")).status).toBe(204);
    expect((await request(h, "DELETE", "0")).status).toBe(204);
    expect((await request(h, "GET", "0")).status).toBe(404);
  });

  it("isolates computers and requires a claimed active account", async () => {
    const a = await claimedHost();
    const b = await claimedHost();
    expect((await request(a, "PUT", "1", new Uint8Array(40).fill(1))).status).toBe(204);
    expect((await request(b, "GET", "1")).status).toBe(404);
    expect((await request(await newHost(), "PUT", "0", new Uint8Array(40).fill(1))).status).toBe(403);
    expect((await SELF.fetch(ORIGIN + "/v1/host/icons/1")).status).toBe(401);
    await env.DB.prepare("UPDATE accounts SET status = 'deleted' WHERE user_id = ?").bind(`user_${a.cid}`).run();
    expect((await request(a, "GET", "1")).status).toBe(403);
  });

  it("bounds slot count and bytes and rejects malformed envelopes", async () => {
    const h = await claimedHost();
    for (const slot of ["128", "999", "-1", "01", "abc"]) {
      expect((await request(h, "PUT", slot, new Uint8Array(40).fill(1))).status).toBe(400);
    }
    expect((await request(h, "PUT", "127", new Uint8Array(65536).fill(1))).status).toBe(204);
    expect((await request(h, "PUT", "0", new Uint8Array(65537).fill(1))).status).toBe(413);
    expect((await request(h, "PUT", "0", new Uint8Array(29).fill(1))).status).toBe(400);
    expect((await request(h, "PUT", "0", new Uint8Array(40))).status).toBe(400);
  });
});
