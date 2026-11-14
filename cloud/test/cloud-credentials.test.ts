// Bring your own cloud: the Boat key each user gives Codync Cloud, sealed at rest, checked before it's kept, never
// handed back, and marked when Boat turns it down later.

import { afterEach, describe, expect, it, vi } from "vitest";
import { wake } from "../src/cloud-machines";
import { open, seal } from "../src/secrets";
import { boat, bothProviders, callWith, calls, cc, cloudComputer, forbidden, KEY, onBothProviders, uid, withKey } from "./cloud-helpers";
import { clerkToken, deliverClerk, env } from "./helpers";

afterEach(() => vi.restoreAllMocks());

const PATH = "/v1/cloud-credentials/boat";
const keyed = (keys: string) => ({ ...env, CLOUD_SECRET_KEYS: keys });
const k = (byte: number) => btoa(String.fromCharCode(...new Uint8Array(32).fill(byte)));
/** The user's Boat key as `GET /v1/cloud-computers` lists it with the providers. */
const credential = async (userId: string) => (await cc("GET", userId)).body.providers.find((p: { provider: string }) => p.provider === "boat").credential;

describe("sealed secrets", () => {
  it("opens only for the owner and purpose it was sealed for", async () => {
    const s = await seal(env, "user_a", "boat", KEY);
    expect(s.sealed).not.toContain("test9abc");
    expect(await open(env, "user_a", "boat", s)).toBe(KEY);
    expect(await open(env, "user_b", "boat", s)).toBeNull();
    expect(await open(env, "user_a", "fly", s)).toBeNull();
    const tampered = { ...s, sealed: s.sealed.slice(0, -2) + (s.sealed.endsWith("AA") ? "AB" : "AA") };
    expect(await open(env, "user_a", "boat", tampered)).toBeNull();
  });

  it("seals with the first key and opens with any, so keys rotate", async () => {
    const old = await seal(keyed(`k1:${k(1)}`), "user_a", "boat", KEY);
    expect(old.keyId).toBe("k1");
    const rotated = keyed(`k2:${k(2)} k1:${k(1)}`);
    expect(await open(rotated, "user_a", "boat", old)).toBe(KEY);
    expect((await seal(rotated, "user_a", "boat", KEY)).keyId).toBe("k2");
    // Dropped once every row was sealed again: what it sealed doesn't open any more.
    expect(await open(keyed(`k2:${k(2)}`), "user_a", "boat", old)).toBeNull();
    await expect(seal(keyed("k1:short"), "user_a", "boat", KEY)).rejects.toThrow("CLOUD_SECRET_KEYS");
  });
});

describe("saving a Boat key", () => {
  it("checks it with Boat, keeps it sealed and only ever shows its hint", async () => {
    const userId = uid();
    const fetch = boat("ready");
    const saved = await cc("PUT", userId, PATH, { key: ` ${KEY} ` });
    expect(saved.status).toBe(200);
    expect(saved.body).toEqual({ credential: { hint: "boat_…9abc", problem: null, blocking: false, updatedAt: expect.any(Number) } });
    expect(fetch.mock.calls.map(([url, init]) => [String(url), (init!.headers as Record<string, string>).authorization])).toEqual([
      ["https://boat.dev/api/v1/sandboxes?limit=1", `Bearer ${KEY}`],
      ["https://boat.dev/api/v1/webhooks", `Bearer ${KEY}`],
    ]);
    const row = await env.DB.prepare("SELECT * FROM cloud_credentials WHERE user_id = ?").bind(userId).first<Record<string, string>>();
    expect(JSON.stringify(row)).not.toContain("test9abc");
    expect(row).toMatchObject({ provider: "boat", hint: "boat_…9abc", problem: null });
    expect(JSON.stringify((await cc("GET", userId)).body)).not.toContain("test9abc");
    expect((await credential(userId)).hint).toBe("boat_…9abc");
  });

  it("refuses what isn't a key, keys Boat turns down, and keys that can run the whole account", async () => {
    const userId = uid();
    const fetch = boat("ready");
    const notAKey = await cc("PUT", userId, PATH, { key: "sk-proj-123" });
    expect(notAKey.body.error).toEqual({ code: "keyRefused", message: "That doesn't look like an API key from this provider." });
    expect(fetch).not.toHaveBeenCalled();

    boat("ready", { "/sandboxes?limit=1": [401, { ok: false, code: "unauthorized", message: "Unauthorized" }] });
    expect((await cc("PUT", userId, PATH, { key: KEY })).body.error.message).toBe("This key wasn't accepted. It may have been revoked.");

    boat("ready", { "/sandboxes?limit=1": forbidden("api_key_action_forbidden") });
    expect((await cc("PUT", userId, PATH, { key: KEY })).body.error.message).toContain("missing a permission Codync needs");

    // An admin key (or one from before scopes) can read the account's webhooks.
    boat("ready", { "/webhooks": [200, { webhooks: [] }] });
    const admin = await cc("PUT", userId, PATH, { key: KEY });
    expect(admin.status).toBe(422);
    expect(admin.body.error.message).toBe("This key can manage your whole account. Make one limited to Codync instead.");
    expect(await credential(userId)).toBeNull();
  });

  it("isn't kept while Boat is busy or failing, which says nothing about the key", async () => {
    const userId = uid();
    boat("ready", { "/sandboxes?limit=1": [503, { code: "no_ready_machine" }] });
    expect((await cc("PUT", userId, PATH, { key: KEY })).body.error.code).toBe("providerUnavailable");
    boat("ready", { "/webhooks": [429, { code: "rate_limited" }] });
    expect((await cc("PUT", userId, PATH, { key: KEY })).status).toBe(503);
    expect(await credential(userId)).toBeNull();
  });

  it("is refused by a cloud that can't seal keys, and for a provider it doesn't know", async () => {
    const fetch = boat("ready");
    expect((await callWith({ CLOUD_SECRET_KEYS: undefined }, "PUT", PATH, { token: await clerkToken(uid()), body: { key: KEY } })).status).toBe(403);
    expect((await cc("PUT", uid(), "/v1/cloud-credentials/fly", { key: KEY })).status).toBe(404);
    expect(fetch).not.toHaveBeenCalled();
  });

  it("takes a replacement only if it reaches the cloud computer, and keeps the key while the computer exists", async () => {
    const { userId, id } = await cloudComputer();
    // The saved key reaches the computer; one from another account doesn't find it.
    boat("ready", { "/sandboxes/bx_test": (key) => (key === KEY ? undefined : [404, { code: "not_found" }]) });
    const other = await cc("PUT", userId, PATH, { key: "boat_live_otheraccount1" });
    expect(other.body.error.message).toBe("This key can't reach your cloud computer.");
    expect((await credential(userId)).hint).toBe("boat_…9abc");

    boat("ready");
    expect((await cc("PUT", userId, PATH, { key: "boat_live_rotated0001" })).body.credential.hint).toBe("boat_…0001");
    const kept = await cc("DELETE", userId, PATH);
    expect(kept.status).toBe(409);

    boat("cancelled");
    expect((await cc("DELETE", userId, `/v1/cloud-computers/${id}`)).status).toBe(200);
    expect((await cc("DELETE", userId, PATH)).status).toBe(200);
    expect(await credential(userId)).toBeNull();
  });

  it("takes a replacement only if it reaches every cloud computer on Boat", async () => {
    const { userId } = await cloudComputer();
    boat("provisioning", { "/sandboxes": [201, { ok: true, sandbox: { id: "bx_two", state: "provisioning" } }] });
    expect((await cc("POST", userId, "/v1/cloud-computers", { provider: "boat" })).status).toBe(200);
    // The saved key reaches both; the new one only the first.
    boat("ready", { "/sandboxes/bx_two": (key) => (key === KEY ? undefined : [404, { code: "not_found" }]) });
    const partial = await cc("PUT", userId, PATH, { key: "boat_live_otheraccount1" });
    expect(partial.body.error).toEqual({ code: "keyRefused", message: "This key can't reach your cloud computer." });
    expect((await credential(userId)).hint).toBe("boat_…9abc");
  });

  it("takes a new key once the computer is gone at Boat and the saved key revoked, so the computer can be deleted", async () => {
    const { userId, id } = await cloudComputer();
    boat("ready", { "/sandboxes/bx_test": (key) => (key === KEY ? [401, { code: "unauthorized" }] : [404, { code: "not_found" }]) });
    expect((await cc("PUT", userId, PATH, { key: "boat_live_newkey00001" })).body.credential.hint).toBe("boat_…0001");
    expect((await cc("DELETE", userId, `/v1/cloud-computers/${id}`)).status).toBe(200);
    expect((await cc("GET", userId)).body.cloudComputers).toEqual([]);
  });
});

describe("a key Boat turns down later", () => {
  it("shows what to fix, stops waking the computer, and clears once Boat takes it again", async () => {
    const { userId, host } = await cloudComputer();
    boat("ready", { "/sandboxes/bx_test": forbidden("api_key_expired") });
    expect(await credential(userId)).toMatchObject({ problem: "This key has expired. Make a new one.", blocking: true });
    // Only the owner can fix it: a wake doesn't keep trying, unlike a busy Boat.
    expect((await wake(env, host.cid, Date.now())).result).toBe("none");
    boat("ready", { "/sandboxes/bx_test": [503, { code: "unavailable" }] });
    expect((await wake(env, host.cid, Date.now())).result).toBe("retry");

    boat("ready");
    expect((await credential(userId)).problem).toBeNull();
  });

  it("keeps a refused action shown though reads still work", async () => {
    const { userId, host } = await cloudComputer();
    boat("archived", { "/sandboxes/bx_test/resume": forbidden("api_key_action_forbidden") });
    expect((await wake(env, host.cid, Date.now())).result).toBe("none");
    expect((await credential(userId)).problem).toContain("missing a permission Codync needs");
  });

  it("lets a computer be created again once the owner fixed what Boat refused", async () => {
    const userId = uid();
    await withKey(userId);
    boat("provisioning", { "/sandboxes": [402, { code: "payment_required" }] });
    const refused = await cc("POST", userId);
    expect(refused.body.error).toEqual({ code: "keyRefused", message: "This account needs an active plan." });
    // Not the key's own fault: Create stays open to try again once the plan is fixed.
    expect(await credential(userId)).toMatchObject({ problem: "This account needs an active plan.", blocking: false });
    boat();
    expect((await cc("POST", userId)).status).toBe(200);
    expect((await credential(userId)).problem).toBeNull();
  });
});

describe("a deleted account", () => {
  it("forgets the key, after its computer was stopped", async () => {
    const { userId } = await cloudComputer();
    // Boat busy: the webhook fails so Clerk sends it again, and the key stays to stop the computer then.
    boat("ready", { "/sandboxes/bx_test/stop": [503, { code: "unavailable" }] });
    const id = `msg_${crypto.randomUUID()}`;
    const payload = { type: "user.deleted", data: { id: userId } };
    expect((await deliverClerk(id, payload)).status).toBe(500);
    expect(await env.DB.prepare("SELECT 1 FROM cloud_credentials WHERE user_id = ?").bind(userId).first()).not.toBeNull();

    const fetch = boat("ready");
    expect((await deliverClerk(id, payload)).status).toBe(200);
    expect(fetch.mock.calls.map(([url, init]) => `${init?.method} ${url}`)).toContain("POST https://boat.dev/api/v1/sandboxes/bx_test/stop");
    expect(await env.DB.prepare("SELECT * FROM cloud_credentials WHERE user_id = ?").bind(userId).first()).toBeNull();
    const machine = await env.DB.prepare("SELECT state FROM cloud_machines WHERE user_id = ?").bind(userId).first<{ state: string }>();
    expect(machine?.state).toBe("deleted");
  });

  it("stops its computers on every provider", async () => {
    const userId = await onBothProviders();
    const fetch = bothProviders("ready");
    expect((await deliverClerk(`msg_${crypto.randomUUID()}`, { type: "user.deleted", data: { id: userId } })).status).toBe(200);
    expect(calls(fetch)).toEqual(
      expect.arrayContaining(["POST https://boat.dev/api/v1/sandboxes/bx_test/stop", "POST https://api.instabox.sh/v1/machines/m_test/suspend"]),
    );
    const { results } = await env.DB.prepare("SELECT state FROM cloud_machines WHERE user_id = ?").bind(userId).all<{ state: string }>();
    expect(results.map((m) => m.state)).toEqual(["deleted", "deleted"]);
  });
});

