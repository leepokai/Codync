// /v1 HTTP API: auth, claims, access requests, grants, webhooks, cron.

import { createScheduledController } from "cloudflare:test";
import { Webhook } from "svix";
import { describe, expect, it } from "vitest";
import worker from "../src/index";
import {
  AUTHORITY,
  call,
  clerkToken,
  devicePath,
  env,
  hostSocket,
  newHost,
  upgrade,
  type Json,
  type TestHost,
} from "./helpers";
import * as ref from "./ref";

let seq = 0;
const uid = (name: string) => `user_${name}_${++seq}_${ref.b64url(ref.random(4))}`;

/** A signed-in user with one registered device key. */
async function newUser(name = "a") {
  const userId = uid(name);
  const token = await clerkToken(userId);
  const key = ref.signKey();
  const r = await call("POST", "/v1/devices", { token, key, body: { name: "iPhone", platform: "ios" } });
  expect(r.status).toBe(200);
  return { userId, token, key, deviceId: r.body.device.deviceId as string, dk: ref.b64url(key.pub) };
}

type User = Awaited<ReturnType<typeof newUser>>;

/** Claims `host` into `user`'s account the way the Mac app does (§4.2 A). */
async function claim(user: User, host: TestHost) {
  const c = await call("POST", "/v1/claims", { token: user.token });
  expect(c.status).toBe(200);
  return call("POST", `/v1/claims/${c.body.claimId}/complete`, { token: user.token, body: claimBody(c.body, user.userId, host) });
}

function claimBody(c: Json, userId: string, host: TestHost) {
  const boxKey = ref.b64url(host.box.pub);
  const canonical = ref.claimCanonical(c.claimId, c.nonce, userId, host.cid, boxKey);
  return {
    computerId: host.cid,
    signKey: ref.b64url(host.sign.pub),
    boxKey,
    name: "Studio",
    platform: "macos",
    device: "macmini",
    version: "2.2.0",
    sig: ref.b64url(ref.sign(host.sign, ref.enc.encode(canonical))),
  };
}

/** Runs §4.2 B up to an approved grant; returns the grant and the SAS both sides computed. */
async function grantAccess(user: User, host: TestHost) {
  const nD = ref.random(32);
  const req = await call("POST", `/v1/computers/${host.cid}/access-requests`, {
    token: user.token,
    key: user.key,
    body: { commit: ref.b64url(ref.sasCommit(user.key.pub, nD)) },
  });
  expect(req.status).toBe(200);
  const id = req.body.requestId as string;
  const nH = ref.random(32);
  expect((await call("POST", `/v1/host/access-requests/${id}/nonce`, { key: host.sign, body: { nonce: ref.b64url(nH) } })).status).toBe(200);
  const seen = await call("GET", `/v1/access-requests/${id}`, { token: user.token });
  expect(seen.body.hostNonce).toBe(ref.b64url(nH));
  expect((await call("POST", `/v1/access-requests/${id}/reveal`, { token: user.token, key: user.key, body: { nonce: ref.b64url(nD) } })).status).toBe(200);
  const state = await call("GET", "/v1/host/state", { key: host.sign });
  const r = state.body.requests.find((x: Json) => x.requestId === id);
  expect(ref.b64url(ref.sasCommit(ref.unb64url(r.deviceKey), ref.unb64url(r.deviceNonce)))).toBe(r.commit);
  const decision = await call("POST", `/v1/host/access-requests/${id}/decision`, { key: host.sign, body: { decision: "approve" } });
  expect(decision.status).toBe(200);
  return {
    requestId: id,
    grantId: decision.body.grantId as string,
    sas: ref.sasCode(host.sign.pub, user.key.pub, nD, nH),
  };
}

describe("basics", () => {
  it("health", async () => {
    expect(await call("GET", "/v1/health")).toEqual({ status: 200, body: { ok: true, version: "2.2.0" } });
  });

  it("errors carry a code and a requestId", async () => {
    const r = await call("GET", "/v1/nope");
    expect(r.status).toBe(404);
    expect(r.body.error.code).toBe("notFound");
    expect(typeof r.body.requestId).toBe("string");
  });
});

describe("Clerk", () => {
  it("accepts a valid session token and records the account", async () => {
    const userId = uid("me");
    const r = await call("GET", "/v1/me", { token: await clerkToken(userId, { email: "kevin@example.com" }) });
    expect(r.status).toBe(200);
    expect(r.body).toMatchObject({ userId, email: "kevin@example.com" });
  });

  it("rejects missing, foreign-issuer, session-less and tampered tokens", async () => {
    const userId = uid("bad");
    expect((await call("GET", "/v1/me")).body.error.code).toBe("unauthenticated");
    expect((await call("GET", "/v1/me", { token: await clerkToken(userId, { iss: "https://evil.example" }) })).status).toBe(401);
    expect((await call("GET", "/v1/me", { token: await clerkToken(userId, { sid: undefined }) })).status).toBe(401);
    expect((await call("GET", "/v1/me", { token: await clerkToken(userId, { exp: Math.floor(Date.now() / 1000) - 60 }) })).status).toBe(401);
    const good = await clerkToken(userId);
    const [h, p, s] = good.split(".");
    const forged = ref.b64url(ref.enc.encode(JSON.stringify({ ...JSON.parse(ref.dec.decode(ref.unb64url(p!))), sub: "user_victim" })));
    expect((await call("GET", "/v1/me", { token: `${h}.${forged}.${s}` })).status).toBe(401);
  });
});

describe("Codync-Sig over HTTP", () => {
  const register = (key: ref.SignKey, o: { nonce?: string; ts?: number; authority?: string } = {}) =>
    call("POST", "/v1/host/register", {
      key,
      ...o,
      body: { boxKey: ref.b64url(ref.boxKey().pub), name: "Mac", platform: "macos", version: "2.2.0" },
    });

  it("rejects a replayed nonce", async () => {
    const key = ref.signKey();
    const nonce = ref.b64url(ref.random(16));
    expect((await register(key, { nonce })).status).toBe(200);
    const again = await register(key, { nonce });
    expect(again.status).toBe(401);
    expect(again.body.error.code).toBe("badSignature");
  });

  it("rejects timestamps outside ±5 minutes and other authorities", async () => {
    const key = ref.signKey();
    expect((await register(key, { ts: Date.now() - 301_000 })).status).toBe(401);
    expect((await register(key, { ts: Date.now() + 301_000 })).status).toBe(401);
    expect((await register(key, { authority: "codync-cloud.example.workers.dev" })).status).toBe(401);
    expect((await register(key, { authority: AUTHORITY.toUpperCase() })).status).toBe(200);
  });

  it("limits new computers per IP", async () => {
    const ip = { "CF-Connecting-IP": "192.0.2.7" };
    const statuses = [];
    for (let i = 0; i < 11; i++) {
      const r = await call("POST", "/v1/host/register", {
        key: ref.signKey(),
        headers: ip,
        body: { boxKey: ref.b64url(ref.boxKey().pub), name: "Mac", platform: "macos", version: "2.2.0" },
      });
      statuses.push(r.status);
    }
    expect(statuses.slice(0, 10).every((s) => s === 200)).toBe(true);
    expect(statuses[10]).toBe(429);
  });

  it("binds the body", async () => {
    const key = ref.signKey();
    const body = ref.enc.encode(JSON.stringify({ boxKey: ref.b64url(ref.boxKey().pub), name: "Mac", platform: "macos", version: "2.2.0" }));
    const sig = ref.signRequest(key, { method: "POST", authority: AUTHORITY, pathAndQuery: "/v1/host/register", body });
    const res = await worker.fetch(
      new Request(`https://${AUTHORITY}/v1/host/register`, {
        method: "POST",
        headers: { "Codync-Sig": sig },
        body: ref.enc.encode(ref.dec.decode(body).replace("Mac", "Evil")),
      }),
      env,
      {} as ExecutionContext,
    );
    expect(res.status).toBe(401);
  });
});

describe("devices", () => {
  it("registers idempotently, lists, and refuses a revoked key", async () => {
    const u = await newUser();
    const again = await call("POST", "/v1/devices", { token: u.token, key: u.key, body: { name: "Kevin's iPhone", platform: "ios" } });
    expect(again.body.device).toMatchObject({ deviceId: u.deviceId, deviceKey: u.dk, name: "Kevin's iPhone" });
    const list = await call("GET", "/v1/devices", { token: u.token });
    expect(list.body.devices).toHaveLength(1);
    expect(list.body.devices[0].revoked).toBe(false);
    expect((await call("DELETE", `/v1/devices/${u.deviceId}`, { token: u.token })).status).toBe(200);
    const revoked = await call("POST", "/v1/devices", { token: u.token, key: u.key, body: { name: "x", platform: "ios" } });
    expect(revoked.status).toBe(403);
  });

  it("requires the device key's signature", async () => {
    const userId = uid("nosig");
    const r = await call("POST", "/v1/devices", { token: await clerkToken(userId), body: { name: "x", platform: "ios" } });
    expect(r.status).toBe(401);
  });

  it("hides other accounts' devices", async () => {
    const a = await newUser();
    const b = await newUser("b");
    expect((await call("DELETE", `/v1/devices/${a.deviceId}`, { token: b.token })).status).toBe(404);
  });
});

describe("claims", () => {
  it("claims a computer with the host's signature and reports ownership to the host", async () => {
    const u = await newUser();
    const host = await newHost();
    const r = await claim(u, host);
    expect(r.status).toBe(200);
    expect(r.body.computer).toMatchObject({ computerId: host.cid, signKey: ref.b64url(host.sign.pub), boxKey: ref.b64url(host.box.pub) });
    const state = await call("GET", "/v1/host/state", { key: host.sign });
    expect(state.body.owner).toEqual({ userId: u.userId, email: `${u.userId}@example.com` });
    const list = await call("GET", "/v1/computers", { token: u.token });
    expect(list.body.computers.map((c: Json) => c.computerId)).toEqual([host.cid]);
    expect(list.body.computers[0].access).toBeNull();
  });

  it("creates the computer row from the signed boxKey when the host never registered", async () => {
    const u = await newUser();
    const host: TestHost = { sign: ref.signKey(), box: ref.boxKey(), cid: "", ver: 0 };
    host.cid = ref.computerId(host.sign.pub);
    const r = await claim(u, host);
    expect(r.status).toBe(200);
    expect(r.body.computer.boxKey).toBe(ref.b64url(host.box.pub));
  });

  it("rejects a bad signature, a wrong computerId, someone else's claim and a used claim", async () => {
    const u = await newUser();
    const other = await newUser("o");
    const host = await newHost();
    const c = (await call("POST", "/v1/claims", { token: u.token })).body;
    const body = claimBody(c, u.userId, host);
    const path = `/v1/claims/${c.claimId}/complete`;
    expect((await call("POST", path, { token: u.token, body: { ...body, boxKey: ref.b64url(ref.boxKey().pub) } })).status).toBe(401);
    expect((await call("POST", path, { token: u.token, body: { ...body, computerId: "A".repeat(22) } })).status).toBe(400);
    expect((await call("POST", path, { token: other.token, body })).status).toBe(404);
    expect((await call("POST", path, { token: u.token, body })).status).toBe(200);
    const reused = await call("POST", path, { token: u.token, body });
    expect(reused.status).toBe(410);
    expect(reused.body.error.code).toBe("claimExpired");
  });

  it("lets exactly one of two accounts win a race", async () => {
    const a = await newUser();
    const b = await newUser("b");
    const host = await newHost();
    const [ca, cb] = await Promise.all([a, b].map(async (u) => (await call("POST", "/v1/claims", { token: u.token })).body));
    const results = await Promise.all([
      call("POST", `/v1/claims/${ca.claimId}/complete`, { token: a.token, body: claimBody(ca, a.userId, host) }),
      call("POST", `/v1/claims/${cb.claimId}/complete`, { token: b.token, body: claimBody(cb, b.userId, host) }),
    ]);
    expect(results.map((r) => r.status).sort()).toEqual([200, 409]);
    expect(results.find((r) => r.status === 409)!.body.error.code).toBe("alreadyClaimed");
  });
});

describe("access requests", () => {
  it("runs commit → host nonce → reveal → approve, and both sides get the same SAS", async () => {
    const u = await newUser();
    const host = await newHost();
    await claim(u, host);
    const { grantId, sas } = await grantAccess(u, host);
    expect(grantId).toMatch(/^grt_/);
    expect(sas).toMatch(/^\d{6}$/);
    const state = await call("GET", "/v1/host/state", { key: host.sign });
    expect(state.body.grants).toEqual([{ grantId, deviceKey: u.dk, deviceName: "iPhone", platform: "ios", scopes: ["control", "screen"] }]);
    const list = await call("GET", "/v1/computers", { token: u.token, key: u.key });
    expect(list.body.computers[0].access).toBe("granted");
    const grants = await call("GET", `/v1/computers/${host.cid}/grants`, { token: u.token });
    expect(grants.body.grants[0]).toMatchObject({ grantId, deviceId: u.deviceId, scopes: ["control", "screen"] });
    const again = await call("POST", `/v1/computers/${host.cid}/access-requests`, {
      token: u.token,
      key: u.key,
      body: { commit: ref.b64url(ref.random(32)) },
    });
    expect(again.status).toBe(409);
  });

  it("enforces order: no reveal before the host nonce, no approve before the reveal, one host nonce", async () => {
    const u = await newUser();
    const host = await newHost();
    await claim(u, host);
    const nD = ref.random(32);
    const req = await call("POST", `/v1/computers/${host.cid}/access-requests`, {
      token: u.token,
      key: u.key,
      body: { commit: ref.b64url(ref.sasCommit(u.key.pub, nD)) },
    });
    const id = req.body.requestId;
    expect((await call("GET", "/v1/computers", { token: u.token, key: u.key })).body.computers[0].access).toBe("pending");
    const early = await call("POST", `/v1/access-requests/${id}/reveal`, { token: u.token, key: u.key, body: { nonce: ref.b64url(nD) } });
    expect(early.status).toBe(409);
    const nonce = (n = ref.random(32)) => call("POST", `/v1/host/access-requests/${id}/nonce`, { key: host.sign, body: { nonce: ref.b64url(n) } });
    expect((await nonce()).status).toBe(200);
    expect((await nonce()).status).toBe(409);
    const approve = () => call("POST", `/v1/host/access-requests/${id}/decision`, { key: host.sign, body: { decision: "approve" } });
    expect((await approve()).status).toBe(409);
    expect((await call("POST", `/v1/access-requests/${id}/reveal`, { token: u.token, key: u.key, body: { nonce: ref.b64url(nD) } })).status).toBe(200);
    expect((await call("POST", `/v1/access-requests/${id}/reveal`, { token: u.token, key: u.key, body: { nonce: ref.b64url(nD) } })).status).toBe(409);
    // A double-click on approve returns the same grant.
    const [first, second] = [await approve(), await approve()];
    expect(first.status).toBe(200);
    expect(second.body).toEqual(first.body);
    expect((await call("GET", `/v1/access-requests/${id}`, { token: u.token })).body).toMatchObject({ status: "approved", grantId: first.body.grantId });
  });

  it("returns 404 for anything belonging to another account", async () => {
    const a = await newUser();
    const b = await newUser("b");
    const host = await newHost();
    await claim(a, host);
    const commit = { commit: ref.b64url(ref.random(32)) };
    const cross = await call("POST", `/v1/computers/${host.cid}/access-requests`, { token: b.token, key: b.key, body: commit });
    expect(cross.status).toBe(404);
    // a's token with b's device key: the device doesn't belong to a.
    expect((await call("POST", `/v1/computers/${host.cid}/access-requests`, { token: a.token, key: b.key, body: commit })).status).toBe(404);
    const own = await call("POST", `/v1/computers/${host.cid}/access-requests`, { token: a.token, key: a.key, body: commit });
    const id = own.body.requestId;
    expect((await call("GET", `/v1/access-requests/${id}`, { token: b.token })).status).toBe(404);
    expect((await call("DELETE", `/v1/access-requests/${id}`, { token: b.token })).status).toBe(404);
    expect((await call("GET", `/v1/computers/${host.cid}/grants`, { token: b.token })).status).toBe(404);
    expect((await call("PATCH", `/v1/computers/${host.cid}`, { token: b.token, body: { name: "mine" } })).status).toBe(404);
    expect((await call("DELETE", `/v1/computers/${host.cid}`, { token: b.token })).status).toBe(404);
    // Another host can't touch this computer's requests either.
    const other = await newHost();
    expect((await call("POST", `/v1/host/access-requests/${id}/nonce`, { key: other.sign, body: { nonce: ref.b64url(ref.random(32)) } })).status).toBe(404);
  });

  it("cancels an earlier pending request instead of reusing its commit", async () => {
    const u = await newUser();
    const host = await newHost();
    await claim(u, host);
    const make = () =>
      call("POST", `/v1/computers/${host.cid}/access-requests`, { token: u.token, key: u.key, body: { commit: ref.b64url(ref.random(32)) } });
    const first = (await make()).body.requestId;
    const second = (await make()).body.requestId;
    expect((await call("GET", `/v1/access-requests/${first}`, { token: u.token })).body.status).toBe("cancelled");
    expect((await call("GET", `/v1/access-requests/${second}`, { token: u.token })).body.status).toBe("pending");
    expect((await call("DELETE", `/v1/access-requests/${second}`, { token: u.token })).status).toBe(200);
    expect((await call("GET", "/v1/host/state", { key: host.sign })).body.requests).toEqual([]);
  });

  it("limits each account to 20 requests an hour", async () => {
    const u = await newUser();
    const host = await newHost();
    await claim(u, host);
    let last = 0;
    for (let i = 0; i < 21; i++) {
      last = (await call("POST", `/v1/computers/${host.cid}/access-requests`, { token: u.token, key: u.key, body: { commit: ref.b64url(ref.random(32)) } })).status;
    }
    expect(last).toBe(429);
  });

  it("denies", async () => {
    const u = await newUser();
    const host = await newHost();
    await claim(u, host);
    const id = (await call("POST", `/v1/computers/${host.cid}/access-requests`, { token: u.token, key: u.key, body: { commit: ref.b64url(ref.random(32)) } })).body.requestId;
    const r = await call("POST", `/v1/host/access-requests/${id}/decision`, { key: host.sign, body: { decision: "deny" } });
    expect(r.body).toEqual({ status: "denied" });
    expect((await call("GET", `/v1/access-requests/${id}`, { token: u.token })).body.status).toBe("denied");
  });
});

describe("revocation reaches the relay", () => {
  it("closes the device socket with 4003, blocks it, and only a new grant unblocks it", async () => {
    const u = await newUser();
    const host = await newHost();
    await claim(u, host);
    const { grantId } = await grantAccess(u, host);
    const hostWs = await hostSocket(host, [{ dk: u.dk, grant: grantId }]);
    const device = (await upgrade(devicePath(host), u.key)).sock!;
    await device.nextT("presence");

    const started = Date.now();
    expect((await call("DELETE", `/v1/computers/${host.cid}/grants/${grantId}`, { token: u.token })).status).toBe(200);
    expect(await device.closed(1000)).toBe(4003);
    expect(Date.now() - started).toBeLessThan(1000);
    await hostWs.nextT("cloud.changed");
    expect((await call("GET", "/v1/host/state", { key: host.sign })).body.grants).toEqual([]);

    // Still listed with the same grant (host hasn't pulled state yet): stays blocked.
    hostWs.send(ref.aclMessage(host.sign, { v: 1, computerId: host.cid, ver: ++host.ver, devices: [{ dk: u.dk, grant: grantId }], offers: [] }));
    await hostWs.nextT("acl.ok");
    expect((await upgrade(devicePath(host), u.key)).status).toBe(403);

    // A new approval → a different grant → unblocked.
    const again = await grantAccess(u, host);
    expect(again.grantId).not.toBe(grantId);
    hostWs.send(ref.aclMessage(host.sign, { v: 1, computerId: host.cid, ver: ++host.ver, devices: [{ dk: u.dk, grant: again.grantId }], offers: [] }));
    await hostWs.nextT("acl.ok");
    expect((await upgrade(devicePath(host), u.key)).status).toBe(101);
  });

  it("a QR pairing of a revoked device unblocks it", async () => {
    const u = await newUser();
    const host = await newHost();
    await claim(u, host);
    const { grantId } = await grantAccess(u, host);
    const hostWs = await hostSocket(host, [{ dk: u.dk, grant: grantId }]);
    expect((await call("DELETE", `/v1/computers/${host.cid}/grants/${grantId}`, { token: u.token })).status).toBe(200);
    await hostWs.nextT("cloud.changed");
    hostWs.send(ref.aclMessage(host.sign, { v: 1, computerId: host.cid, ver: ++host.ver, devices: [], offers: [] }));
    await hostWs.nextT("acl.ok");
    expect((await upgrade(devicePath(host), u.key)).status).toBe(403);

    // The host paired it again locally (grant null): the host is the authority.
    hostWs.send(ref.aclMessage(host.sign, { v: 1, computerId: host.cid, ver: ++host.ver, devices: [{ dk: u.dk, grant: null }], offers: [] }));
    await hostWs.nextT("acl.ok");
    expect((await upgrade(devicePath(host), u.key)).status).toBe(101);
  });

  it("revoking the device revokes its grants everywhere", async () => {
    const u = await newUser();
    const host = await newHost();
    await claim(u, host);
    const { grantId } = await grantAccess(u, host);
    const hostWs = await hostSocket(host, [{ dk: u.dk, grant: grantId }]);
    const device = (await upgrade(devicePath(host), u.key)).sock!;
    expect((await call("DELETE", `/v1/devices/${u.deviceId}`, { token: u.token })).status).toBe(200);
    expect(await device.closed()).toBe(4003);
    await hostWs.nextT("cloud.changed");
    expect((await call("GET", "/v1/host/state", { key: host.sign })).body.grants).toEqual([]);
  });

  it("unclaiming clears the owner and grants", async () => {
    const u = await newUser();
    const host = await newHost();
    await claim(u, host);
    await grantAccess(u, host);
    expect((await call("POST", "/v1/host/unclaim", { key: host.sign, body: {} })).status).toBe(200);
    const state = await call("GET", "/v1/host/state", { key: host.sign });
    expect(state.body).toEqual({ owner: null, grants: [], requests: [] });
    expect((await call("GET", "/v1/computers", { token: u.token })).body.computers).toEqual([]);
  });

  it("the host's own revoke marks the grant revoked", async () => {
    const u = await newUser();
    const host = await newHost();
    await claim(u, host);
    const { grantId } = await grantAccess(u, host);
    expect((await call("POST", `/v1/host/grants/${grantId}/revoke`, { key: host.sign, body: {} })).status).toBe(200);
    expect((await call("GET", `/v1/computers/${host.cid}/grants`, { token: u.token })).body.grants).toEqual([]);
    const other = await newHost();
    expect((await call("POST", `/v1/host/grants/${grantId}/revoke`, { key: other.sign, body: {} })).status).toBe(404);
  });
});

describe("Clerk webhook", () => {
  const deliver = (id: string, payload: unknown, secret = env.CLERK_WEBHOOK_SECRET!) => {
    const body = JSON.stringify(payload);
    const now = new Date();
    return call("POST", "/v1/webhooks/clerk", {
      body: payload,
      headers: {
        "svix-id": id,
        "svix-timestamp": String(Math.floor(now.getTime() / 1000)),
        "svix-signature": new Webhook(secret).sign(id, now, body),
      },
    });
  };

  it("rejects bad signatures", async () => {
    const r = await deliver("msg_bad", { type: "user.deleted", data: { id: "x" } }, `whsec_${btoa("0".repeat(24))}`);
    expect(r.status).toBe(401);
  });

  it("deletes the account, revokes everything, and processes each event once", async () => {
    const u = await newUser();
    const host = await newHost();
    await claim(u, host);
    await grantAccess(u, host);
    const id = `msg_${ref.b64url(ref.random(8))}`;
    expect((await deliver(id, { type: "user.deleted", data: { id: u.userId } })).status).toBe(200);
    expect((await call("GET", "/v1/me", { token: u.token })).body.error.code).toBe("accountDeleted");
    expect((await call("GET", "/v1/host/state", { key: host.sign })).body).toEqual({ owner: null, grants: [], requests: [] });
    expect((await deliver(id, { type: "user.deleted", data: { id: u.userId } })).status).toBe(200);
    const n = await env.DB.prepare("SELECT COUNT(*) AS n FROM processed_events WHERE event_id = ?").bind(id).first<{ n: number }>();
    expect(n?.n).toBe(1);
    const audits = await env.DB.prepare("SELECT COUNT(*) AS n FROM audit_events WHERE action = 'account.delete' AND target = ?")
      .bind(u.userId)
      .first<{ n: number }>();
    expect(audits?.n).toBe(1);
  });
});

describe("cron", () => {
  it("expires pending requests and clears their nonces", async () => {
    const u = await newUser();
    const host = await newHost();
    await claim(u, host);
    const id = (await call("POST", `/v1/computers/${host.cid}/access-requests`, { token: u.token, key: u.key, body: { commit: ref.b64url(ref.random(32)) } })).body.requestId;
    await call("POST", `/v1/host/access-requests/${id}/nonce`, { key: host.sign, body: { nonce: ref.b64url(ref.random(32)) } });
    await env.DB.prepare("UPDATE access_requests SET expires_at = ? WHERE id = ?").bind(Date.now() - 1, id).run();
    expect((await call("GET", `/v1/access-requests/${id}`, { token: u.token })).body.status).toBe("expired");
    const decision = await call("POST", `/v1/host/access-requests/${id}/decision`, { key: host.sign, body: { decision: "approve" } });
    expect(decision.body.error.code).toBe("requestExpired");
    await worker.scheduled(createScheduledController(), env);
    const row = await env.DB.prepare("SELECT status, host_nonce FROM access_requests WHERE id = ?").bind(id).first();
    expect(row).toEqual({ status: "expired", host_nonce: null });
  });
});
