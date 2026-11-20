// Codync Cloud's own analytics: the host's switch, saved when its owner turns it and with each sleep request, and the
// wakes the Worker reports to PostHog (a stand-in host the fetch spy catches), only for a machine whose host shares
// analytics.

import { runDurableObjectAlarm, runInDurableObject } from "cloudflare:test";
import { describe, expect, it, vi } from "vitest";
import { capture, computerTag } from "../src/analytics";
import { wake } from "../src/cloud-machines";
import { reportWoken, WakeSchedule } from "../src/relay-wake";
import { WAKE_RETRY_MS } from "../src/relay-wire";
import { alarmAfter, boat, cloudComputer, forbidden, relay, spyFetch, stoppedBoat } from "./cloud-helpers";
import { call, devicePath, env, hostSocket, sleep, socket, type Json } from "./helpers";
import * as ref from "./ref";

const BATCH = "https://posthog.test/batch/";

/** The batches sent to PostHog. */
const batches = (spy: ReturnType<typeof spyFetch>): Json[] =>
  spy.mock.calls.filter(([url]) => String(url) === BATCH).map(([, init]) => JSON.parse(init!.body as string));

/** The events sent about `userId`'s machines: a relay from an earlier test may still be reporting its own. */
const sent = (spy: ReturnType<typeof spyFetch>, userId: string): Json[] =>
  batches(spy)
    .flatMap((b) => b.batch)
    .filter((e: Json) => e.distinct_id === userId);

/** What the `cloud_computer_woken` events sent about `userId`'s machines came to, and why. */
const woken = (spy: ReturnType<typeof spyFetch>, userId: string) => sent(spy, userId).map((e) => [e.properties.outcome, e.properties.cause]);

const share = (id: string, on: number | null) => env.DB.prepare("UPDATE cloud_machines SET analytics = ? WHERE id = ?").bind(on, id).run();
const shared = async (id: string) => (await env.DB.prepare("SELECT analytics FROM cloud_machines WHERE id = ?").bind(id).first<{ analytics: number | null }>())?.analytics;

describe("the host's analytics switch", () => {
  it("is saved with each sleep request, a refused one too, and kept when the host doesn't say", async () => {
    const { host, device, id } = await cloudComputer();
    expect(await shared(id)).toBeNull();
    boat("archiving");
    const dev = await socket(devicePath(host), device);
    const busy = await call("POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: null, analytics: true } });
    expect(busy.body.error.code).toBe("conflict");
    expect(await shared(id)).toBe(1);
    dev.ws.close();
    await sleep(100);
    expect((await call("POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: null } })).status).toBe(200);
    expect(await shared(id)).toBe(1);
    await call("POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: null, analytics: false } });
    expect(await shared(id)).toBe(0);
  });

  it("is saved at once when its owner turns it", async () => {
    const { host, id } = await cloudComputer();
    expect((await call("POST", "/v1/host/analytics", { key: host.sign, body: { analytics: true } })).status).toBe(200);
    expect(await shared(id)).toBe(1);
    await call("POST", "/v1/host/analytics", { key: host.sign, body: { analytics: false } });
    expect(await shared(id)).toBe(0);
    expect((await call("POST", "/v1/host/analytics", { key: host.sign, body: {} })).status).toBe(400);
    expect(await shared(id)).toBe(0);
  });
});

describe("wakes", () => {
  it("come to what the provider answered", async () => {
    const { host, id } = await cloudComputer();
    const outcome = async () => (await wake(env, host.cid, Date.now())).woken?.outcome;
    stoppedBoat();
    expect(await outcome()).toBe("started");
    boat("archived", { "/sandboxes/bx_test/resume": [429, { code: "rate_limited" }] });
    expect(await outcome()).toBe("failed");
    boat("archived", { "/sandboxes/bx_test/resume": forbidden("api_key_action_forbidden") });
    expect(await outcome()).toBe("refused");
    // Its own snapshot expired: started from its backup instead.
    await env.DB.prepare("UPDATE cloud_machines SET backup = 'codync-backup', backup_at = 1 WHERE id = ?").bind(id).run();
    spyFetch().mockImplementation(async (url, init) => {
      if (init?.method === "GET") return Response.json({ sandbox: { id: "bx_test", state: "archived" } });
      if (String(url).endsWith("/resume")) return Response.json({ code: "snapshot_data_expired", message: "gone" }, { status: 409 });
      return Response.json({ sandbox: { id: "bx_restored", state: "provisioning" } }, { status: 201 });
    });
    expect(await outcome()).toBe("restored");
    // Only a start counts: a machine already starting asks the provider nothing.
    boat("provisioning");
    expect(await outcome()).toBeUndefined();
  });

  it("are reported with what made the relay wake it", async () => {
    const { userId, host, id } = await cloudComputer();
    await share(id, 1);
    const spy = stoppedBoat();
    const w = await wake(env, host.cid, Date.now());
    await reportWoken(env, w.woken!, "routine", Date.now());
    expect(batches(spy).map((b) => b.api_key)).toEqual([expect.stringMatching(/^phc_/)]);
    expect(sent(spy, userId)).toEqual([
      {
        event: "cloud_computer_woken",
        distinct_id: userId,
        timestamp: expect.stringMatching(/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/),
        uuid: expect.stringMatching(/^[0-9a-f-]{36}$/),
        properties: {
          outcome: "started",
          cause: "routine",
          provider: "boat",
          computer: await computerTag(host.cid),
          $lib: "codync-cloud",
          env: "dev",
          $process_person_profile: true,
          $geoip_disable: true,
        },
      },
    ]);
  });

  it("send nothing for a machine whose host doesn't share analytics, or with no PostHog host", async () => {
    const spy = spyFetch();
    const m = { analytics: 1, user_id: "user_quiet", provider: "boat" as const, computer_id: null };
    const e = { event: "cloud_computer_woken", outcome: "started", cause: "device" } as const;
    await capture(env, { ...m, analytics: null }, e, Date.now());
    await capture(env, { ...m, analytics: 0 }, e, Date.now());
    await capture({ ...env, POSTHOG_HOST: undefined }, m, e, Date.now());
    expect(sent(spy, m.user_id)).toEqual([]);
  });

  it("never let a send that failed break a wake", async () => {
    spyFetch().mockRejectedValueOnce(new Error("offline"));
    const m = { analytics: 1, user_id: "user_x", provider: "boat" as const, computer_id: null };
    await expect(capture(env, m, { event: "cloud_computer_woken", outcome: "started", cause: "device" }, Date.now())).resolves.toBeUndefined();
  });

  it("key a computer as its host does", async () => {
    // The host's own vector (`computer_tag_matches_the_iphone` in host/src/analytics.rs).
    expect(await computerTag("AAECAwQFBgcICQoLDA0ODw")).toBe("2d6878bee20aba04");
  });
});

describe("a sleeping computer's relay", () => {
  /** A cloud computer whose host shares analytics, asleep: what its relay reports once `wakeIt` wakes it. */
  async function reportedWake(wakeIt: (c: Awaited<ReturnType<typeof cloudComputer>>) => Promise<unknown>) {
    const c = await cloudComputer();
    await share(c.id, 1);
    c.hostWs.ws.close();
    await sleep(100);
    const spy = stoppedBoat();
    await wakeIt(c);
    await vi.waitFor(() => expect(sent(spy, c.userId)).toHaveLength(1));
    return woken(spy, c.userId);
  }

  it("reports why it woke the computer: the time its host asked for, or an access request", async () => {
    const routine = await reportedWake(async ({ host, id }) => {
      // The host's own sleep request says it shares analytics; an earlier wake's cause is still in the relay's meta.
      await share(id, null);
      await runInDurableObject(relay(host), (_, state) => {
        state.storage.sql.exec("INSERT OR REPLACE INTO meta(k, v) VALUES ('wakeCause', 'device')");
      });
      expect((await call("POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: Date.now() - 1, analytics: true } })).status).toBe(200);
      // A time already past fires the alarm at once; run it here in case it hasn't yet.
      await runDurableObjectAlarm(relay(host));
    });
    expect(routine).toEqual([["started", "routine"]]);
    const access = await reportedWake(({ host }) =>
      relay(host).fetch("https://do/internal/changed", { method: "POST", headers: { "X-Codync-Internal": "1" }, body: JSON.stringify({ wake: true }) }),
    );
    expect(access).toEqual([["started", "access"]]);
  });

  it("keeps when to wake it and why together, and reads a cause it doesn't know as its routine", () => {
    const kv = new Map<string, string>();
    const schedule = new WakeSchedule({ get: (k) => kv.get(k) ?? null, set: (k, v) => kv.set(k, v) });
    schedule.set(100, "device");
    expect(schedule.sooner(200, "retry")).toBe(false);
    expect(schedule.sooner(50, "retry")).toBe(true);
    expect(schedule.due(49)).toBeNull();
    expect(schedule.due(50)).toBe("retry");
    expect(schedule.pending()).toBe(0);
    // A relay's meta from before causes were kept.
    kv.set("wakeAt", "10").delete("wakeCause");
    expect(schedule.due(10)).toBe("routine");
  });

  it("reports how long a wake took and how many starts, once", async () => {
    const { userId, host, device, hostWs, id } = await cloudComputer();
    await share(id, 1);
    hostWs.ws.close();
    await sleep(100);
    const busy = boat("archived", { "/sandboxes/bx_test/resume": [429, { code: "rate_limited" }] });
    await socket(devicePath(host), device);
    await vi.waitFor(() => expect(woken(busy, userId)).toEqual([["failed", "device"]]));
    // Retried from the alarm.
    await sleep(100);
    const retried = stoppedBoat();
    await alarmAfter(host, WAKE_RETRY_MS + 1_000);
    await vi.waitFor(() => expect(woken(retried, userId)).toEqual([["started", "retry"]]));
    // As if the first try had been 42 s ago.
    await runInDurableObject(relay(host), (_, state) => {
      state.storage.sql.exec("UPDATE meta SET v = CAST(v AS INTEGER) - 42000 WHERE k = 'wakeStart'");
    });

    const devices = [{ dk: ref.b64url(device.pub), grant: null }];
    const back = spyFetch();
    const ws = await hostSocket(host, devices);
    await vi.waitFor(() => expect(sent(back, userId)).toHaveLength(1));
    const [awake] = sent(back, userId);
    expect(awake.event).toBe("cloud_computer_awake");
    expect(awake.properties).toMatchObject({ attempts: 2, provider: "boat", computer: await computerTag(host.cid), $lib: "codync-cloud" });
    expect(awake.properties.seconds).toBeGreaterThanOrEqual(42);
    expect(awake.properties.seconds).toBeLessThan(52);

    // Back again without a wake: nothing to report.
    ws.ws.close();
    await sleep(100);
    const again = spyFetch();
    await hostSocket(host, devices);
    await sleep(200);
    expect(sent(again, userId)).toEqual([]);
  });
});
