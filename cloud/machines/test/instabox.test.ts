import { describe, expect, it } from "vitest";
import { instabox } from "../src";
import { setupCommand } from "../src/instabox";

const ID = "m_agq3spfhoqtq2moj3xn6hwty2m";

describe("instabox", () => {
  it("creates a machine without the account's Secrets, from the template every plan may use", () => {
    const r = instabox.create({ name: "codync-User_AB12", size: "small", group: "codync", files: [], setupScript: "true", autoStopSeconds: 600 }, "clm_1");
    expect(r).toEqual({
      method: "POST",
      path: "/v1/machines",
      headers: { "Idempotency-Key": "clm_1" },
      body: { name: "codync-user_ab12", templateId: "agent-browser", vault: false, secrets: [], autoSuspendAfterSeconds: 1800, metadata: { codync: "cloud" } },
    });
    expect(instabox.sleep(ID, "k1")).toEqual({ method: "POST", path: `/v1/machines/${ID}/suspend`, headers: { "Idempotency-Key": "k1" } });
    expect(instabox.wake(ID, "k2")).toEqual({ method: "POST", path: `/v1/machines/${ID}/start`, headers: { "Idempotency-Key": "k2" } });
    expect(instabox.destroy(ID, "k3").headers).toEqual({ "Idempotency-Key": "k3", "X-Confirm-Delete": ID });
    expect(instabox.backup).toBeUndefined();
    expect(instabox.deleteHistory).toBeUndefined();
    expect(instabox.keyPattern.test(`ibk_${"a2".repeat(26)}`)).toBe(true);
    expect(instabox.keyPattern.test("ibk_short")).toBe(false);
  });

  it("reads machines and the operations changes start; running is usable only once ready", () => {
    const machine = (state: string, ready?: boolean) => instabox.machine({ status: 200, body: { id: ID, state, runtime: { ready } } });
    expect(machine("running", true)).toEqual({ ok: true, value: { provider: "instabox", id: ID, state: "running" } });
    expect(machine("running", false)).toMatchObject({ value: { state: "starting" } });
    expect(machine("suspended")).toMatchObject({ value: { state: "stopped" } });
    expect(machine("suspending")).toMatchObject({ value: { state: "stopping" } });
    expect(machine("error")).toMatchObject({ value: { state: "failed" } });
    const op = (type: string, state = "queued", error: unknown = null) => instabox.machine({ status: 202, body: { id: "op_1", machineId: ID, type, state, error } });
    expect(op("create")).toEqual({ ok: true, value: { provider: "instabox", id: ID, state: "starting" } });
    expect(op("suspend")).toMatchObject({ value: { state: "stopping" } });
    expect(op("start", "failed", { code: "no_capacity", message: "full", retryable: true })).toMatchObject({ ok: false, code: "unavailable", retryable: true });
    expect(instabox.machine({ status: 200, body: {} })).toMatchObject({ ok: false, code: "badResponse" });
  });

  it("maps InstaBox's errors into Codync's words", () => {
    const err = (status: number, code: string, retryable = false) => instabox.machine({ status, body: { error: { code, message: code, retryable } } });
    expect(err(401, "revoked_key")).toMatchObject({ code: "keyRejected", needsOwner: true });
    expect(err(401, "expired_key")).toMatchObject({ code: "keyExpired", needsOwner: true });
    expect(err(403, "missing_scope")).toMatchObject({ code: "keyMissingAction", needsOwner: true });
    expect(err(403, "template_not_allowed")).toMatchObject({ code: "planRequired", needsOwner: true });
    expect(err(402, "credit_exhausted")).toMatchObject({ code: "freeHoursUsed", needsOwner: true });
    expect(err(402, "spend_limit_reached")).toMatchObject({ code: "planRequired", needsOwner: true });
    expect(err(429, "quota_exceeded")).toMatchObject({ code: "machineLimit", needsOwner: true, retryable: false });
    // These two say each time whether a retry may work: too many at once, or the plan's limit.
    expect(err(429, "quota_exceeded", true)).toMatchObject({ code: "busy", needsOwner: false, retryable: true });
    expect(err(429, "concurrency_limit")).toMatchObject({ code: "machineLimit", needsOwner: true, retryable: false });
    expect(err(429, "concurrency_limit", true)).toMatchObject({ code: "busy", needsOwner: false, retryable: true });
    expect(err(409, "operation_conflict")).toMatchObject({ code: "busy", retryable: true, needsOwner: false });
    expect(err(409, "machine_cooldown")).toMatchObject({ code: "busy", retryable: true });
    expect(err(503, "no_capacity", true)).toMatchObject({ code: "unavailable", retryable: true });
    expect(err(404, "not_found")).toMatchObject({ code: "notFound", needsOwner: false });
    expect(instabox.machine({ status: 401, body: null })).toMatchObject({ code: "keyRejected", detail: "http401", needsOwner: true });
    expect(instabox.deleted({ status: 404, body: null })).toEqual({ ok: true, value: null });
    expect(instabox.done({ status: 200, body: { stdout: "", exitCode: 0 } })).toEqual({ ok: true, value: null });
    expect(instabox.done({ status: 200, body: { stdout: "", exitCode: 1 } })).toMatchObject({ ok: false, detail: "exit1" });
  });

  it("asks the key's workspace whether it is an admin's", () => {
    expect(instabox.checkKey()).toEqual({ method: "GET", path: "/v1/me" });
    expect(instabox.checkAdmin({ status: 200, body: { orgId: "org_1" } })).toEqual({ method: "GET", path: "/v1/orgs/org_1/invites" });
    expect(instabox.checkAdmin({ status: 200, body: { org: { id: "org_2" } } })).toMatchObject({ path: "/v1/orgs/org_2/invites" });
    expect(instabox.checkAdmin({ status: 200, body: {} })).toBeNull();
    expect(instabox.isAdmin({ status: 200, body: { invites: [] } })).toBe(true);
    expect(instabox.isAdmin({ status: 403, body: { error: { code: "missing_scope" } } })).toBe(false);
    expect(instabox.isAdmin({ status: 503, body: null })).toBeNull();
  });

  it("sets a running machine up with one exec that writes the files and starts the script detached", () => {
    const setup = { files: [{ path: ".codync/managed.json", content: '{"claimId":"clm_1","note":"it\'s ü"}' }], setupScript: "echo set up > done.txt\n" };
    const r = instabox.setup!(ID, setup);
    expect(r).toMatchObject({ method: "POST", path: `/v1/machines/${ID}/exec`, body: { cmd: "bash -s", timeoutSeconds: 60, noHistory: true } });
    // Each file and the script go in as base64 (any quote or byte survives the shell), decoded where they belong.
    const decoded = (target: string) => {
      const m = new RegExp(`printf %s '([A-Za-z0-9+/=]+)' \\| base64 -d > '?${target.replace(".", "\\.")}'?\n`).exec(setupCommand(setup));
      return m && new TextDecoder().decode(Uint8Array.from(atob(m[1]!), (ch) => ch.charCodeAt(0)));
    };
    expect(decoded(".codync/managed.json")).toBe(setup.files[0]!.content);
    expect(decoded(".codync-setup.sh")).toBe(setup.setupScript);
    expect(setupCommand(setup)).toMatch(/^set -eu\numask 077\ncd ~\nmkdir -p '\.codync'\n/);
    expect(setupCommand(setup)).toMatch(/\nsetsid nohup bash \.codync-setup\.sh > \.codync-setup\.log 2>&1 < \/dev\/null &\n$/);
  });
});
