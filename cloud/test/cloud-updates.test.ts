// Codync Cloud versions and snapshots: app versions per device, urgent and soft host updates, emergency
// floors, the release list, backups, and deleting a computer's saved history.

import { afterEach, describe, expect, it, vi } from "vitest";
import { syncReleases } from "../src/host-releases";
import { saveBackups, wake } from "../src/cloud-machines";
import { boat, callWith, calls, cc, cloudComputer, spyFetch } from "./cloud-helpers";
import { call, clerkToken, devicePath, env, socket } from "./helpers";
import * as ref from "./ref";

afterEach(() => vi.restoreAllMocks());

/** A cloud computer whose owner has a registered phone that reported `app`. */
async function withPhone(app: string) {
  const cc = await cloudComputer();
  const token = await clerkToken(cc.userId);
  expect((await call("POST", "/v1/devices", { token, key: cc.device, body: { name: "iPhone", platform: "ios" } })).status).toBe(200);
  const listed = await call("GET", "/v1/computers", { token, key: cc.device, headers: { "Codync-App": app } });
  return { ...cc, token, listed };
}

describe("app versions", () => {
  it("keeps each device's version and shows which provider runs a cloud computer", async () => {
    const { listed, device } = await withPhone("ios/2.26.0; minHost=2.25.0");
    expect(listed.body.computers[0].cloud).toEqual({ provider: "boat", providerName: "Boat", region: "EU", state: "starting" });
    const row = await env.DB.prepare("SELECT app_version, min_host FROM devices WHERE sign_pub = ?").bind(ref.b64url(device.pub)).first();
    expect(row).toEqual({ app_version: "2.26.0", min_host: "2.25.0" });
  });

  it("refuses an app below an emergency floor, with the reason", async () => {
    const floor = { APP_FLOOR_IOS: "2.21.0", APP_FLOOR_REASON: "A security fix." };
    const old = await callWith(floor, "GET", "/v1/health", { app: "ios/2.20.9" });
    expect(old.status).toBe(426);
    expect(old.body.error).toMatchObject({ code: "appUpdateRequired", message: "A security fix." });
    expect((await callWith(floor, "GET", "/v1/health", { app: "ios/2.21.0" })).status).toBe(200);
    expect((await callWith(floor, "GET", "/v1/health", { app: "android/1.0.0" })).status).toBe(200);
    expect((await callWith(floor, "GET", "/v1/health", {})).status).toBe(200);
  });
});

describe("cloud host updates", () => {
  it("orders the exact host an app needs, at once, and tells a connected host", async () => {
    const { host, hostWs, device } = await withPhone("ios/2.26.0; minHost=2.25.0");
    const state = await call("GET", "/v1/host/state", { key: host.sign });
    expect(state.body.update).toEqual({ version: "2.25.0", force: true });
    await socket(devicePath(host), device, { "Codync-App": "ios/2.26.0; minHost=2.25.0" });
    await hostWs.nextT("cloud.changed");
  });

  it("orders the oldest published release above the host an app needs when that version was never released", async () => {
    const releases = (rows: string[][]) =>
      env.DB.batch([
        env.DB.prepare("DELETE FROM host_releases"),
        ...rows.map(([v, m]) => env.DB.prepare("INSERT INTO host_releases(version, min_app) VALUES (?, ?)").bind(v, m)),
      ]);
    // `dev` bumped 2.25.0 to 2.25.1 before main released it.
    await releases([
      ["2.24.0", "2.18.0"],
      ["2.25.1", "2.21.0"],
      ["2.26.0", "2.26.0"],
    ]);
    const { host } = await withPhone("ios/2.26.0; minHost=2.25.0");
    expect((await call("GET", "/v1/host/state", { key: host.sign })).body.update).toEqual({ version: "2.25.1", force: true });
    await releases([]);
  });

  it("raises the order to an emergency floor; a self-hosted computer below it may not register", async () => {
    const { host } = await withPhone("ios/2.26.0; minHost=2.25.0");
    const state = await callWith({ HOST_FLOOR: "2.30.0" }, "GET", "/v1/host/state", { key: host.sign });
    expect(state.body.update).toEqual({ version: "2.30.0", force: true });
    const register = (key: ref.SignKey) =>
      callWith({ HOST_FLOOR: "2.30.0", HOST_FLOOR_REASON: "Update now." }, "POST", "/v1/host/register", {
        key,
        body: { boxKey: ref.b64url(ref.boxKey().pub), name: "Mac", platform: "macos", device: "laptop", version: "2.29.0" },
      });
    const own = await register(ref.signKey());
    expect(own.status).toBe(426);
    expect(own.body.error).toMatchObject({ code: "hostUpdateRequired", message: "Update now." });
    expect((await register(host.sign)).status).toBe(200);
  });

  it("offers the newest release every recent device can use instead of sleeping", async () => {
    const { host } = await withPhone("ios/2.22.0; minHost=2.20.0");
    await env.DB.batch(
      [
        ["2.24.0", "2.18.0"],
        ["2.25.0", "2.21.0"],
        ["2.26.0", "2.26.0"],
      ].map(([v, m]) => env.DB.prepare("INSERT INTO host_releases(version, min_app) VALUES (?, ?)").bind(v, m)),
    );
    const fetch = boat("archiving");
    const res = await call("POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: null } });
    expect(res.body).toEqual({ update: { version: "2.25.0", force: false } });
    expect(calls(fetch)).not.toContain("POST https://boat.dev/api/v1/sandboxes/bx_test/stop");
  });

  it("keeps each release its channel marks latest, with its MIN_APP, and drops any above it", async () => {
    await env.DB.batch([
      env.DB.prepare("DELETE FROM host_releases"),
      ...[
        ["2.24.0", "2.18.0"],
        ["2.25.0", "2.20.0"],
        ["2.27.0", "2.21.0"],
      ].map(([v, m]) => env.DB.prepare("INSERT INTO host_releases(version, min_app) VALUES (?, ?)").bind(v, m)),
    ]);
    // The tests run with the dev cloud's settings, so on its dev channel.
    const fetch = spyFetch().mockImplementation(async () => Response.json({ version: "2.25.0", minApp: "2.21.0" }));
    await syncReleases(env);
    expect(String(fetch.mock.calls.at(-1)?.[0])).toBe("https://dev-releases.codync.dev/latest/download/codync-host-compat.json");
    const { results } = await env.DB.prepare("SELECT version, min_app FROM host_releases ORDER BY version").all();
    expect(results).toEqual([
      { version: "2.24.0", min_app: "2.18.0" },
      { version: "2.25.0", min_app: "2.21.0" },
    ]);

    spyFetch().mockImplementation(async () => Response.json({ version: "2.26.0; DROP", minApp: "2.21.0" }));
    await expect(syncReleases(env)).rejects.toThrow("invalid");
    // Without a channel (main): GitHub's releases.
    const github = spyFetch().mockImplementation(async () => new Response("", { status: 404 }));
    await expect(syncReleases({ ...env, HOST_RELEASES_URL: undefined })).rejects.toThrow("404");
    expect(String(github.mock.calls.at(-1)?.[0])).toBe("https://github.com/leepokai/Codync/releases/latest/download/codync-host-compat.json");
  });
});

describe("snapshots", () => {
  it("deletes an awake computer's saved history, but not while it sleeps", async () => {
    const { userId, id } = await cloudComputer();
    const fetch = boat("ready");
    expect((await cc("DELETE", userId, `/v1/cloud-computers/${id}/snapshots`)).status).toBe(200);
    expect(calls(fetch)).toContain("DELETE https://boat.dev/api/v1/sandboxes/bx_test/snapshots");
    boat("archived");
    const asleep = await cc("DELETE", userId, `/v1/cloud-computers/${id}/snapshots`);
    expect(asleep.status).toBe(409);
  });

  it("backs up a machine asleep for ten days, and brings it back from the backup once its own snapshot expired", async () => {
    const { id, host } = await cloudComputer();
    const old = Date.now() - 11 * 86_400_000;
    await env.DB.prepare("UPDATE cloud_machines SET state = 'stopped', updated_at = ? WHERE id = ?").bind(old, id).run();
    const saving = boat("archived");
    await saveBackups(env, Date.now());
    expect(calls(saving)).toEqual(["POST https://boat.dev/api/v1/saved-snapshots"]);
    const row = await env.DB.prepare("SELECT backup, backup_at FROM cloud_machines WHERE id = ?").bind(id).first<{ backup: string; backup_at: number }>();
    expect(row?.backup).toMatch(/^codync-[0-9a-f-]{36}$/);

    spyFetch().mockImplementation(async (url, init) => {
      const u = String(url);
      if (init?.method === "GET") return Response.json({ sandbox: { id: "bx_test", state: "archived" } });
      if (u.endsWith("/resume")) return Response.json({ code: "snapshot_data_expired", message: "gone" }, { status: 409 });
      return Response.json({ sandbox: { id: "bx_restored", state: "provisioning" } }, { status: 201 });
    });
    expect((await wake(env, host.cid, Date.now())).result).toBe("started");
    const after = await env.DB.prepare("SELECT provider_id FROM cloud_machines WHERE id = ?").bind(id).first();
    expect(after).toEqual({ provider_id: "bx_restored" });
  });

  it("times the backup from when it asked to sleep, though it was seen stopped only later", async () => {
    const { userId, id } = await cloudComputer();
    const old = Date.now() - 11 * 86_400_000;
    await env.DB.prepare("UPDATE cloud_machines SET state = 'stopping', updated_at = ? WHERE id = ?").bind(old, id).run();
    boat("archived");
    expect((await cc("GET", userId)).body.cloudComputers[0].state).toBe("stopped");
    const saving = boat("archived");
    await saveBackups(env, Date.now());
    expect(calls(saving)).toEqual(["POST https://boat.dev/api/v1/saved-snapshots"]);
  });

  it("keeps a computer whose backup couldn't be deleted yet, so deleting it can be tried again", async () => {
    const { userId, id } = await cloudComputer();
    await env.DB.prepare("UPDATE cloud_machines SET backup = 'codync-kept', backup_at = ? WHERE id = ?").bind(Date.now(), id).run();
    const busy = boat("archived", { "/saved-snapshots/codync-kept": [429, { code: "rate_limited" }] });
    expect((await cc("DELETE", userId, `/v1/cloud-computers/${id}`)).body.error.code).toBe("providerUnavailable");
    expect(calls(busy)).toEqual(["DELETE https://boat.dev/api/v1/saved-snapshots/codync-kept"]);
    boat("archived", { "/saved-snapshots/codync-kept": [401, { code: "unauthorized" }] });
    expect((await cc("DELETE", userId, `/v1/cloud-computers/${id}`)).body.error.code).toBe("keyRefused");
    expect((await cc("GET", userId)).body.cloudComputers).toHaveLength(1);
    const gone = boat("cancelled");
    expect((await cc("DELETE", userId, `/v1/cloud-computers/${id}`)).status).toBe(200);
    expect(calls(gone)).toContain("DELETE https://boat.dev/api/v1/saved-snapshots/codync-kept");
    expect((await cc("GET", userId)).body.cloudComputers).toEqual([]);
  });

  it("times the backup from when it went to sleep, whatever it was renamed since", async () => {
    const { userId, id } = await cloudComputer();
    const old = Date.now() - 11 * 86_400_000;
    await env.DB.prepare("UPDATE cloud_machines SET state = 'stopped', updated_at = ? WHERE id = ?").bind(old, id).run();
    boat("archived");
    expect((await cc("PATCH", userId, `/v1/cloud-computers/${id}`, { name: "Research" })).status).toBe(200);
    const saving = boat("archived");
    await saveBackups(env, Date.now());
    expect(calls(saving)).toEqual(["POST https://boat.dev/api/v1/saved-snapshots"]);
  });
});
