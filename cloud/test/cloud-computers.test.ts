// Codync Cloud: cloud computers on Boat (stubbed), the host's self-claim from its ticket, sleep and wake.

import { runDurableObjectAlarm } from "cloudflare:test";
import { afterEach, describe, expect, it, vi } from "vitest";
import { boat, calls, cc, cloudComputer, hostClaim, KEY, relay, stoppedBoat, uid, withKey } from "./cloud-helpers";
import { call, clerkToken, devicePath, newHost, sleep, socket } from "./helpers";
import * as ref from "./ref";

afterEach(() => vi.restoreAllMocks());

describe("cloud computers", () => {
  it("is refused for accounts off the allowlist", async () => {
    const fetch = boat();
    const other = uid();
    expect((await cc("GET", other, "user_other")).body).toEqual({ cloudComputer: null, available: false, providers: [] });
    expect((await cc("POST", other, "user_other")).status).toBe(403);
    expect(fetch).not.toHaveBeenCalled();
    const open = (await cc("GET", uid())).body;
    expect(open).toMatchObject({
      cloudComputer: null,
      available: true,
      providers: [
        { provider: "boat", name: "Boat", region: "EU", keyPrompt: "boat_…", setup: { page: "https://boat.dev/dashboard" }, credential: null },
        { provider: "instabox", name: "InstaBox", region: "US", keyPrompt: "ibk_…", setup: { page: "https://dashboard.instabox.sh/dashboard/api-keys" }, credential: null },
      ],
    });
    expect(open.providers[0].setup.commands).toContain("boat api-key create codync --env <id> --ttl never --actions sandbox.create,");
    expect(open.providers[1].setup.commands).toContain("instabox keys create --name codync --scope machines:read,machines:write,machines:delete");
  });

  it("needs the user's own Boat key", async () => {
    const userId = uid();
    const fetch = boat();
    const refused = await cc("POST", userId);
    expect(refused.status).toBe(409);
    expect(refused.body.error.message).toBe("Connect a cloud provider first.");
    expect((await cc("POST", userId, userId, "/v1/cloud-computers", { provider: "nimbus" })).status).toBe(400);
    expect(fetch).not.toHaveBeenCalled();
  });

  it("creates a sandbox with a claim ticket, which the host completes itself, then deletes it", async () => {
    const userId = uid();
    await withKey(userId);
    const fetch = boat();
    const created = await cc("POST", userId, userId, "/v1/cloud-computers", { provider: "boat" });
    expect(created.status).toBe(200);
    expect(created.body.cloudComputer).toMatchObject({ provider: "boat", providerName: "Boat", region: "EU", state: "starting", computerId: null, snapshots: true });

    const [url, init] = fetch.mock.calls[0]!;
    expect(url).toBe("https://boat.dev/api/v1/sandboxes");
    const headers = init!.headers as Record<string, string>;
    expect(headers.authorization).toBe(`Bearer ${KEY}`);
    const sent = JSON.parse(init!.body as string);
    expect(sent).toMatchObject({ type: "small", environment: "codync", noEnv: true, monitoring: false, ttlSeconds: null });
    expect(sent.files[0].path).toBe(".codync/managed.json");
    const ticket = JSON.parse(sent.files[0].content);
    expect(ticket).toMatchObject({ userId, approval: "auto" });
    expect(headers["Idempotency-Key"]).toBe(ticket.claimId);
    expect(sent.setupScript).toContain("codync-host install");

    expect((await cc("POST", userId)).body.cloudComputer.cloudComputerId).toBe(created.body.cloudComputer.cloudComputerId);
    expect(fetch).toHaveBeenCalledTimes(1);

    const host = await newHost();
    const other = await newHost();
    expect((await hostClaim(other, { ...ticket, nonce: "wrong" })).status).toBe(401);
    expect((await hostClaim(host, ticket)).status).toBe(200);
    expect((await hostClaim(host, ticket)).body.error.code).toBe("claimExpired");

    boat("ready");
    const live = await cc("GET", userId);
    expect(live.body.cloudComputer).toMatchObject({ state: "running", computerId: host.cid });

    const token = await clerkToken(userId);
    expect((await call("DELETE", `/v1/computers/${host.cid}`, { token })).status).toBe(409);

    const destroy = boat("cancelled");
    expect((await cc("DELETE", userId)).status).toBe(200);
    const [dUrl, dInit] = destroy.mock.lastCall!;
    expect(dUrl).toBe("https://boat.dev/api/v1/sandboxes/bx_test");
    expect((dInit!.headers as Record<string, string>)["X-Ascii-Confirm-Delete"]).toBe("bx_test");
    expect((await cc("GET", userId)).body.cloudComputer).toBeNull();
    const list = await call("GET", "/v1/computers", { token });
    expect(list.body.computers).toEqual([]);
  });
});

describe("sleep and wake", () => {
  it("stops an idle cloud computer, but not while a device is connected", async () => {
    const { host, device } = await cloudComputer();
    const dev = await socket(devicePath(host), device);
    const fetch = boat("archiving");
    const busy = await call("POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: null } });
    expect(busy.body.error.code).toBe("conflict");
    // A relay from an earlier test may still be finishing a wake: only this computer's stop counts.
    const stop = "POST https://boat.dev/api/v1/sandboxes/bx_test/stop";
    expect(calls(fetch)).not.toContain(stop);
    dev.ws.close();
    await sleep(100);
    expect((await call("POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: null } })).status).toBe(200);
    expect(calls(fetch).filter((c) => c === stop)).toHaveLength(1);
    expect((await call("GET", "/v1/host/sleep", { key: host.sign })).body.state).toBe("stopping");
    expect((await call("POST", "/v1/host/sleep", { key: (await newHost()).sign, body: {} })).status).toBe(404);
  });

  it("shows it offline as soon as it asks to sleep, though its socket is still open", async () => {
    const { host, device } = await cloudComputer();
    boat("archiving");
    expect((await call("POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: null } })).status).toBe(200);
    const dev = await socket(devicePath(host), device);
    expect((await dev.nextT("presence")).online).toBe(false);
  });

  it("keeps it online when its owner's key can't stop it", async () => {
    const { host, device } = await cloudComputer();
    boat("ready", { "/sandboxes/bx_test/stop": [401, { code: "unauthorized" }] });
    const refused = await call("POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: null } });
    expect(refused.body.error.code).toBe("keyRefused");
    const dev = await socket(devicePath(host), device);
    expect((await dev.nextT("presence")).online).toBe(true);
  });

  it("takes the host back when the machine never stopped", async () => {
    const { host, device, hostWs } = await cloudComputer();
    boat("archiving");
    expect((await call("POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: null } })).status).toBe(200);
    boat("ready");
    const dev = await socket(devicePath(host), device);
    expect((await dev.nextT("presence")).online).toBe(false);
    expect((await dev.nextT("presence")).online).toBe(true);
    expect((await hostWs.nextT("open")).dk).toBe(ref.b64url(device.pub));
  });

  it("wakes it when a device connects while it sleeps", async () => {
    const { host, device, hostWs } = await cloudComputer();
    hostWs.ws.close();
    await sleep(100);
    const fetch = stoppedBoat();
    await socket(devicePath(host), device);
    await vi.waitFor(() => expect(calls(fetch)).toContain("POST https://boat.dev/api/v1/sandboxes/bx_test/resume"));
    // A host process that survived the stop learns it was woken and reconnects.
    await vi.waitFor(async () => expect((await call("GET", "/v1/host/sleep", { key: host.sign })).body.state).toBe("starting"));
  });

  it("wakes it for an access request, which it has to answer", async () => {
    const { userId, host, hostWs } = await cloudComputer();
    hostWs.ws.close();
    await sleep(100);
    const token = await clerkToken(userId);
    const key = ref.signKey();
    expect((await call("POST", "/v1/devices", { token, key, body: { name: "Mac", platform: "macos" } })).status).toBe(200);
    const fetch = stoppedBoat();
    const req = await call("POST", `/v1/computers/${host.cid}/access-requests`, { token, key, body: { commit: ref.b64url(ref.sasCommit(key.pub, ref.random(32))) } });
    expect(req.status).toBe(200);
    await vi.waitFor(() => expect(calls(fetch)).toContain("POST https://boat.dev/api/v1/sandboxes/bx_test/resume"), { timeout: 5000 });
  });

  it("wakes it at the time it asked for", async () => {
    const { host, hostWs } = await cloudComputer();
    hostWs.ws.close();
    await sleep(100);
    const fetch = stoppedBoat();
    expect((await call("POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: Date.now() - 1 } })).status).toBe(200);
    // A time already past fires the alarm at once; run it here in case it hasn't yet.
    await runDurableObjectAlarm(relay(host));
    await vi.waitFor(() => expect(calls(fetch)).toContain("POST https://boat.dev/api/v1/sandboxes/bx_test/resume"));
  });

  it("asks again later while the machine is still stopping", async () => {
    const { host, device, hostWs } = await cloudComputer();
    hostWs.ws.close();
    await sleep(100);
    const fetch = boat("archiving");
    await socket(devicePath(host), device);
    await vi.waitFor(() => expect(calls(fetch)).toEqual(["GET https://boat.dev/api/v1/sandboxes/bx_test"]));
    // The retry runs from the alarm once its time has come.
    await sleep(100);
    const retried = stoppedBoat();
    vi.useFakeTimers({ toFake: ["Date"], now: Date.now() + 21_000 });
    try {
      await runDurableObjectAlarm(relay(host));
    } finally {
      vi.useRealTimers();
    }
    expect(calls(retried)).toContain("POST https://boat.dev/api/v1/sandboxes/bx_test/resume");
  });

  it("wakes it again once the minute is up when it went back to sleep right after a wake", async () => {
    const { host, device, hostWs } = await cloudComputer();
    hostWs.ws.close();
    await sleep(100);
    const first = stoppedBoat();
    const dev = await socket(devicePath(host), device);
    await vi.waitFor(() => expect(calls(first)).toContain("POST https://boat.dev/api/v1/sandboxes/bx_test/resume"));
    dev.ws.close();
    await sleep(100);
    // Asked again within the minute: nothing now, but it isn't forgotten.
    const second = stoppedBoat();
    await socket(devicePath(host), device);
    await sleep(100);
    expect(calls(second)).toEqual([]);
    vi.useFakeTimers({ toFake: ["Date"], now: Date.now() + 61_000 });
    try {
      await runDurableObjectAlarm(relay(host));
    } finally {
      vi.useRealTimers();
    }
    expect(calls(second)).toContain("POST https://boat.dev/api/v1/sandboxes/bx_test/resume");
  });
});

