// Codync Cloud: cloud computers on Boat (stubbed), the host's self-claim from its ticket, sleep and wake.

import { runDurableObjectAlarm } from "cloudflare:test";
import { afterEach, describe, expect, it, vi } from "vitest";
import { STOP_SETTLE_MS } from "../src/cloud-machines";
import { WAKE_RETRY_MS } from "../src/relay-wire";
import { alarmAfter, boat, bothProviders, callWith, calls, cc, cloudComputer, hostClaim, instaboxApi, KEY, onBothProviders, relay, spyFetch, stoppedBoat, uid, withInstaboxKey, withKey } from "./cloud-helpers";
import { call, clerkToken, devicePath, env, newHost, sleep, socket } from "./helpers";
import * as ref from "./ref";

afterEach(() => vi.restoreAllMocks());

describe("cloud computers", () => {
  it("is open to every account, on a cloud that can seal their keys", async () => {
    const fetch = boat();
    const noKeys = async (method: string) => callWith({ CLOUD_SECRET_KEYS: undefined }, method, "/v1/cloud-computers", { token: await clerkToken(uid()) });
    expect((await noKeys("GET")).body).toEqual({ cloudComputers: [], available: false, limit: 3, providers: [] });
    expect((await noKeys("POST")).status).toBe(403);
    expect(fetch).not.toHaveBeenCalled();
    const open = (await cc("GET", uid())).body;
    expect(open).toMatchObject({
      cloudComputers: [],
      available: true,
      providers: [
        { provider: "boat", name: "Boat", region: "EU", keyPrompt: "boat_…", setup: { page: "https://boat.dev/dashboard" }, credential: null },
        { provider: "instabox", name: "InstaBox", region: "US", keyPrompt: "ibk_…", setup: { page: "https://dashboard.instabox.sh/dashboard/api-keys" }, credential: null },
      ],
    });
    expect(open.providers[0].setup.commands).toContain("boat api-key create codync --env <id> --ttl never --actions sandbox.create,");
    expect(open.providers[1].setup.commands).toContain("instabox keys create --name codync --scope machines:read,machines:write,machines:delete");
    // Only a provider with a free plan offers one.
    expect(open.providers[0].offer).toBeUndefined();
    expect(open.providers[1].offer).toEqual({
      label: "Get a free sandbox",
      detail: "InstaBox's free plan includes one machine (4 vCPU, 8 GiB).",
      url: "https://instabox.sh/pricing",
    });
  });

  it("needs the user's own Boat key", async () => {
    const userId = uid();
    const fetch = boat();
    const refused = await cc("POST", userId);
    expect(refused.status).toBe(409);
    expect(refused.body.error.message).toBe("Connect a cloud provider first.");
    expect((await cc("POST", userId, "/v1/cloud-computers", { provider: "nimbus" })).status).toBe(400);
    expect(fetch).not.toHaveBeenCalled();
  });

  it("creates a sandbox with a claim ticket, which the host completes itself, then deletes it", async () => {
    const userId = uid();
    await withKey(userId);
    const fetch = boat();
    const created = await cc("POST", userId, "/v1/cloud-computers", { provider: "boat", name: " Lab " });
    expect(created.status).toBe(200);
    expect(created.body.cloudComputer).toMatchObject({ name: "Lab", provider: "boat", providerName: "Boat", region: "EU", state: "starting", computerId: null, snapshots: true });
    const id = created.body.cloudComputer.cloudComputerId as string;

    const [url, init] = fetch.mock.calls[0]!;
    expect(url).toBe("https://boat.dev/api/v1/sandboxes");
    const headers = init!.headers as Record<string, string>;
    expect(headers.authorization).toBe(`Bearer ${KEY}`);
    const sent = JSON.parse(init!.body as string);
    expect(sent).toMatchObject({ type: "small", environment: "codync", noEnv: true, monitoring: false, ttlSeconds: null });
    expect(sent.name).toMatch(/^codync-[a-z0-9-]+$/);
    expect(sent.files[0].path).toBe(".codync/managed.json");
    const ticket = JSON.parse(sent.files[0].content);
    expect(ticket).toMatchObject({ userId, approval: "auto" });
    expect(headers["Idempotency-Key"]).toBe(ticket.claimId);
    expect(sent.setupScript).toContain("/.local/bin/codync-host\" install");

    expect(fetch).toHaveBeenCalledTimes(1);

    const host = await newHost();
    const other = await newHost();
    expect((await hostClaim(other, { ...ticket, nonce: "wrong" })).status).toBe(401);
    const claimed = await hostClaim(host, ticket);
    expect(claimed.status).toBe(200);
    // Its computer takes the name its user gave it, not the host's.
    expect(claimed.body.computer.name).toBe("Lab");
    expect((await hostClaim(host, ticket)).body.error.code).toBe("claimExpired");

    boat("ready");
    const live = await cc("GET", userId);
    expect(live.body.cloudComputers).toMatchObject([{ state: "running", computerId: host.cid }]);

    const token = await clerkToken(userId);
    expect((await call("DELETE", `/v1/computers/${host.cid}`, { token })).status).toBe(409);

    const destroy = boat("cancelled");
    expect((await cc("DELETE", userId, "/v1/cloud-computers/cm_nope")).status).toBe(404);
    expect((await cc("DELETE", userId, `/v1/cloud-computers/${id}`)).status).toBe(200);
    const [dUrl, dInit] = destroy.mock.lastCall!;
    expect(dUrl).toBe("https://boat.dev/api/v1/sandboxes/bx_test");
    expect((dInit!.headers as Record<string, string>)["X-Ascii-Confirm-Delete"]).toBe("bx_test");
    expect((await cc("GET", userId)).body.cloudComputers).toEqual([]);
    const list = await call("GET", "/v1/computers", { token });
    expect(list.body.computers).toEqual([]);
  });
});

describe("several cloud computers", () => {
  it("runs one per provider side by side, each with its own name, up to the limit", async () => {
    const userId = uid();
    await withKey(userId);
    await withInstaboxKey(userId);
    boat();
    expect((await cc("POST", userId)).body.error.message).toBe("Choose the provider to create it on.");
    const first = await cc("POST", userId, "/v1/cloud-computers", { provider: "boat" });
    expect(first.body.cloudComputer.name).toBe("Cloud computer");
    instaboxApi();
    const second = await cc("POST", userId, "/v1/cloud-computers", { provider: "instabox", name: "Build box" });
    expect(second.status).toBe(200);
    const listed = await cc("GET", userId);
    expect(listed.body.cloudComputers.map((m: { name: string; provider: string }) => [m.name, m.provider])).toEqual([
      ["Cloud computer", "boat"],
      ["Build box", "instabox"],
    ]);

    expect((await cc("POST", userId, "/v1/cloud-computers", { provider: "instabox" })).status).toBe(200);
    const over = await cc("POST", userId, "/v1/cloud-computers", { provider: "instabox" });
    expect(over.status).toBe(409);
    expect(over.body.error.message).toBe("You can have up to 3 cloud computers. Delete one to make another.");

    // A key with a computer on it stays; the other provider's key goes once its computers are gone.
    const removeBoat = await cc("DELETE", userId, "/v1/cloud-credentials/boat");
    expect(removeBoat.body.error.message).toBe("Delete your cloud computers on it first: Codync needs the key to put them to sleep.");
  });

  it("renames one, as its computer too, and its host is told", async () => {
    const { userId, host, id } = await cloudComputer();
    const token = await clerkToken(userId);
    expect((await cc("PATCH", userId, `/v1/cloud-computers/${id}`, { name: "  " })).status).toBe(400);
    const renamed = await cc("PATCH", userId, `/v1/cloud-computers/${id}`, { name: "Research" });
    expect(renamed.body.cloudComputer.name).toBe("Research");
    expect((await call("GET", "/v1/computers", { token })).body.computers[0].name).toBe("Research");
    // The host takes it from its state, and registering again keeps it.
    expect((await call("GET", "/v1/host/state", { key: host.sign })).body.name).toBe("Research");
    await call("POST", "/v1/host/register", {
      key: host.sign,
      body: { boxKey: ref.b64url(host.box.pub), name: "Codync Cloud", platform: "linux", version: "2.25.0" },
    });
    expect((await call("GET", "/v1/computers", { token })).body.computers[0].name).toBe("Research");
  });

  it("lists every one while a provider can't be reached", async () => {
    const userId = await onBothProviders();
    bothProviders("ready", async () => {
      throw new TypeError("fetch failed");
    });
    const listed = await cc("GET", userId);
    expect(listed.status).toBe(200);
    expect(listed.body.cloudComputers.map((m: { name: string }) => m.name)).toEqual(["Cloud computer", "Build box"]);
    // The one Boat answered for is up to date.
    expect(listed.body.cloudComputers[0].state).toBe("running");
  });

  it("is only its own account's to rename or delete", async () => {
    const { id } = await cloudComputer();
    const other = uid();
    const fetch = boat("ready");
    expect((await cc("PATCH", other, `/v1/cloud-computers/${id}`, { name: "Mine" })).status).toBe(404);
    expect((await cc("DELETE", other, `/v1/cloud-computers/${id}/snapshots`)).status).toBe(404);
    expect((await cc("DELETE", other, `/v1/cloud-computers/${id}`)).status).toBe(404);
    expect(fetch).not.toHaveBeenCalled();
  });
});

/** The host asks to sleep and Boat takes the stop, but still reports the machine running when a device connects:
 * the wake waits for the stop, and the machine stays recorded stopping, so its host keeps waiting too. */
async function wakeWhileStopping({ host, device, hostWs }: Awaited<ReturnType<typeof cloudComputer>>) {
  boat("archiving");
  expect((await call("POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: null } })).status).toBe(200);
  const fetch = boat("ready");
  const dev = await socket(devicePath(host), device);
  expect((await dev.nextT("presence")).online).toBe(false);
  await vi.waitFor(() => expect(calls(fetch)).toContain("GET https://boat.dev/api/v1/sandboxes/bx_test"));
  await sleep(100);
  expect(hostWs.msgs.some((m) => m.t === "open")).toBe(false);
  expect((await call("GET", "/v1/host/sleep", { key: host.sign })).body.state).toBe("stopping");
  return dev;
}

describe("sleep and wake", () => {
  it("stops an idle cloud computer, but not while a device is connected", async () => {
    const { host, device, id } = await cloudComputer();
    const dev = await socket(devicePath(host), device);
    const fetch = boat("archiving");
    const state = async () => (await call("GET", "/v1/host/sleep", { key: host.sign })).body.state;
    const before = await state();
    // Recorded `stopping` before its relay is asked to take the host offline, and put back when the relay refuses or
    // can't be reached.
    const asked: unknown[] = [];
    const RELAY = {
      idFromName: (name: string) => env.RELAY.idFromName(name),
      get: (doId: DurableObjectId) => ({
        fetch: async (url: string, init: RequestInit) => {
          asked.push((await env.DB.prepare("SELECT state FROM cloud_machines WHERE id = ?").bind(id).first())?.state);
          return env.RELAY.get(doId).fetch(url, init);
        },
      }),
    } as unknown as typeof env.RELAY;
    const busy = await callWith({ RELAY }, "POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: null } });
    expect(busy.body.error.code).toBe("conflict");
    expect(asked).toEqual(["stopping"]);
    expect(await state()).toBe(before);
    const unreachable = {
      idFromName: RELAY.idFromName,
      get: () => ({ fetch: () => Promise.reject(new Error("relay unreachable")) }),
    } as unknown as typeof env.RELAY;
    expect((await callWith({ RELAY: unreachable }, "POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: null } })).status).toBe(500);
    expect(await state()).toBe(before);
    // A relay from an earlier test may still be finishing a wake: only this computer's stop counts.
    const stop = "POST https://boat.dev/api/v1/sandboxes/bx_test/stop";
    expect(calls(fetch)).not.toContain(stop);
    dev.ws.close();
    await sleep(100);
    expect((await call("POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: null } })).status).toBe(200);
    expect(calls(fetch).filter((c) => c === stop)).toHaveLength(1);
    expect(await state()).toBe("stopping");
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

  it("takes the host back at the first wake when the provider was too busy to stop it", async () => {
    const { host, device } = await cloudComputer();
    boat("ready", { "/sandboxes/bx_test/stop": [429, { code: "rate_limited" }] });
    const refused = await call("POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: null } });
    expect(refused.body.error.code).toBe("providerUnavailable");
    expect((await call("GET", "/v1/host/sleep", { key: host.sign })).body.state).toBe("running");
    const dev = await socket(devicePath(host), device);
    expect((await dev.nextT("presence")).online).toBe(false);
    expect((await dev.nextT("presence")).online).toBe(true);
  });

  it("doesn't take the host back for a wake that read its machine before it asked to sleep again", async () => {
    const { host, device } = await cloudComputer();
    // A stop turned down as busy: recorded running, its host connected but offline.
    boat("ready", { "/sandboxes/bx_test/stop": [429, { code: "rate_limited" }] });
    await call("POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: null } });
    // A wake reads it as running; the host asks to sleep again before the provider answers.
    let answer!: () => void;
    const held = new Promise<void>((r) => (answer = r));
    const fetch = spyFetch().mockImplementation(async (_url, init) => {
      if (init?.method !== "POST") await held;
      return Response.json({ ok: true, sandbox: { id: "bx_test", state: init?.method === "POST" ? "archiving" : "ready" } });
    });
    const wakeBody = JSON.stringify({ wake: true });
    const waking = relay(host).fetch("https://do/internal/changed", { method: "POST", headers: { "X-Codync-Internal": "1" }, body: wakeBody });
    await vi.waitFor(() => expect(calls(fetch)).toContain("GET https://boat.dev/api/v1/sandboxes/bx_test"));
    expect((await call("POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: null } })).status).toBe(200);
    answer();
    await waking;
    // Still running only because its stop hasn't taken effect yet: the host stays offline.
    const dev = await socket(devicePath(host), device);
    expect((await dev.nextT("presence")).online).toBe(false);
    await dev.none((m) => m.t === "presence" && m.online === true);
  });

  it("takes the host back when the machine never stopped", async () => {
    const computer = await cloudComputer();
    const dev = await wakeWhileStopping(computer);
    // Its stop had all the time it needs, and the wake asks again.
    await env.DB.prepare("UPDATE cloud_machines SET updated_at = updated_at - ? WHERE id = ?").bind(STOP_SETTLE_MS, computer.id).run();
    await alarmAfter(computer.host, WAKE_RETRY_MS + 1_000);
    expect((await dev.nextT("presence")).online).toBe(true);
    expect((await computer.hostWs.nextT("open")).dk).toBe(ref.b64url(computer.device.pub));
  });

  it("keeps a wake that came while its stop was taking effect, and starts it once stopped", async () => {
    const computer = await cloudComputer();
    await wakeWhileStopping(computer);
    const retried = stoppedBoat();
    await alarmAfter(computer.host, WAKE_RETRY_MS + 1_000);
    expect(calls(retried)).toContain("POST https://boat.dev/api/v1/sandboxes/bx_test/resume");
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
    await alarmAfter(host, 21_000);
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
    await alarmAfter(host, 61_000);
    expect(calls(second)).toContain("POST https://boat.dev/api/v1/sandboxes/bx_test/resume");
  });
});

