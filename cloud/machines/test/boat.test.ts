import { describe, expect, it } from "vitest";
import { boat } from "../src";

describe("boat", () => {
  it("creates a protected, unmonitored sandbox that never stops by itself", () => {
    const r = boat.create({ name: "codync-u1", size: "small", files: [{ path: ".codync/managed.json", content: "{}" }], setupScript: "true" }, "k1");
    expect(r).toEqual({
      method: "POST",
      path: "/sandboxes",
      headers: { "Idempotency-Key": "k1" },
      body: {
        name: "codync-u1",
        type: "small",
        ttlSeconds: null,
        noEnv: true,
        monitoring: false,
        files: [{ path: ".codync/managed.json", content: "{}" }],
        setupScript: "true",
      },
    });
    expect(boat.destroy("bx_23456789").headers).toEqual({ "X-Ascii-Confirm-Delete": "bx_23456789" });
  });

  it("maps Boat's states and errors", () => {
    expect(boat.machine({ status: 202, body: { ok: true, sandbox: { id: "bx_23456789", state: "provisioning" } } })).toEqual({
      ok: true,
      value: { provider: "boat", id: "bx_23456789", state: "starting" },
    });
    expect(boat.machine({ status: 200, body: { sandbox: { id: "bx_23456789", state: "archived" } } })).toMatchObject({ value: { state: "stopped" } });
    expect(boat.machine({ status: 429, body: { ok: false, code: "rate_limited", message: "Start limit" } })).toEqual({
      ok: false,
      code: "rate_limited",
      message: "Start limit",
      retryable: true,
      needsOwner: false,
    });
    expect(boat.machine({ status: 402, body: null })).toMatchObject({ ok: false, code: "http402", retryable: false, needsOwner: true });
    expect(boat.machine({ status: 402, body: { code: "payment_required" } })).toMatchObject({ code: "http402", needsOwner: true });
    expect(boat.machine({ status: 401, body: { code: "unauthorized" } })).toMatchObject({ needsOwner: true });
    expect(boat.machine({ status: 403, body: { code: "api_key_expired" } })).toMatchObject({ needsOwner: true });
    expect(boat.machine({ status: 400, body: { code: "unknown_environment" } })).toMatchObject({ needsOwner: true });
    expect(boat.machine({ status: 409, body: { code: "idempotency_in_progress" } })).toMatchObject({ needsOwner: false });
    expect(boat.deleted({ status: 404, body: null })).toEqual({ ok: true, value: null });
  });

  it("tells an admin key from a scoped one, and says nothing when Boat doesn't", () => {
    expect(boat.isAdmin({ status: 200, body: { webhooks: [] } })).toBe(true);
    expect(boat.isAdmin({ status: 403, body: { code: "api_key_action_forbidden" } })).toBe(false);
    expect(boat.isAdmin({ status: 404, body: null })).toBe(false);
    expect(boat.isAdmin({ status: 403, body: { code: "forbidden" } })).toBeNull();
    expect(boat.isAdmin({ status: 429, body: { code: "rate_limited" } })).toBeNull();
    expect(boat.isAdmin({ status: 503, body: null })).toBeNull();
  });

  it("builds snapshot requests: history delete, backups, a machine from a backup", () => {
    expect(boat.deleteSnapshots("bx_23456789")).toEqual({
      method: "DELETE",
      path: "/sandboxes/bx_23456789/snapshots",
      headers: { "X-Ascii-Confirm-Delete": "bx_23456789" },
    });
    expect(boat.saveBackup("bx_23456789", "codync-cm1")).toEqual({ method: "POST", path: "/saved-snapshots", body: { sandboxId: "bx_23456789", name: "codync-cm1" } });
    expect(boat.deleteBackup("codync-cm1")).toEqual({ method: "DELETE", path: "/saved-snapshots/codync-cm1" });
    expect(boat.fromBackup({ name: "codync-u1", size: "small" }, "codync-cm1", "k2").body).toMatchObject({ from: "codync-cm1", noEnv: true, ttlSeconds: null });
    expect(boat.fromBackup({ name: "codync-u1", size: "small", group: "codync" }, "codync-cm1", "k2").body).toMatchObject({ environment: "codync" });
    expect(boat.create({ name: "codync-u1", size: "small", group: "codync", files: [], setupScript: "true" }, "k1").body).toMatchObject({ environment: "codync" });
    expect(boat.done({ status: 202, body: {} })).toEqual({ ok: true, value: null });
    expect(boat.done({ status: 409, body: { code: boat.SNAPSHOT_EXPIRED } })).toMatchObject({ ok: false, code: "snapshot_data_expired" });
  });
});
