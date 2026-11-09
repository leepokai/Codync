// Codync Cloud on InstaBox (stubbed): a key checked against workspace admins, a machine that gets its claim ticket
// once it runs (InstaBox takes nothing at create), and sleep as a pause.

import { describe, expect, it } from "vitest";
import { setUpPending, wake } from "../src/cloud-machines";
import { call, clerkToken, deliverClerk, env, newHost } from "./helpers";
import { calls, cc, hostClaim, IBX_KEY, instaboxApi, instaboxTicket, uid, withInstaboxKey } from "./cloud-helpers";

const API = "https://api.instabox.sh";
const PATH = "/v1/cloud-credentials/instabox";

const row = (userId: string) =>
  env.DB.prepare("SELECT * FROM cloud_machines WHERE user_id = ?").bind(userId).first<{ setup_at: number | null; state: string; claim_id: string }>();

/** A user with an InstaBox key and a cloud computer InstaBox is still creating. */
async function creating() {
  const userId = uid();
  await withInstaboxKey(userId);
  instaboxApi("creating");
  await cc("POST", userId, "/v1/cloud-computers", { provider: "instabox" });
  return userId;
}

/** InstaBox as `instaboxApi`, except that every exec never reaches it. */
function execUnreachable() {
  const spy = instaboxApi("running");
  const answer = spy.getMockImplementation()!;
  return spy.mockImplementation(async (url, init) => {
    if (String(url).endsWith("/exec")) throw new TypeError("fetch failed");
    return answer(url, init);
  });
}

const execs = (spy: ReturnType<typeof instaboxApi>) => calls(spy).filter((c) => c.endsWith("/exec"));

describe("an InstaBox key", () => {
  it("is kept once InstaBox takes it and it isn't a workspace admin's", async () => {
    const userId = uid();
    const fetch = instaboxApi();
    expect((await cc("PUT", userId, PATH, { key: "boat_live_test9abc" })).body.error.message).toBe("That doesn't look like an API key from this provider.");
    expect(fetch).not.toHaveBeenCalled();

    const saved = await cc("PUT", userId, PATH, { key: IBX_KEY });
    expect(saved.body).toEqual({ credential: { hint: "ibk_…a2a2", problem: null, blocking: false, updatedAt: expect.any(Number) } });
    expect(calls(fetch)).toEqual([`GET ${API}/v1/me`, `GET ${API}/v1/orgs/org_1/invites`]);
    expect((fetch.mock.calls[0]![1]!.headers as Record<string, string>).authorization).toBe(`Bearer ${IBX_KEY}`);

    instaboxApi("running", { "GET /v1/orgs/org_1/invites": [200, { invites: [] }] });
    const admin = await cc("PUT", uid(), PATH, { key: IBX_KEY });
    expect(admin.body.error.message).toBe("This key can manage your whole account. Make one limited to Codync instead.");
    instaboxApi("running", { "GET /v1/me": [401, { error: { code: "revoked_key", message: "revoked" } }] });
    expect((await cc("PUT", uid(), PATH, { key: IBX_KEY })).body.error.message).toBe("This key wasn't accepted. It may have been revoked.");
  });
});

describe("a cloud computer on InstaBox", () => {
  it("gets its claim ticket once it runs, then sleeps as a pause and wakes with a start", async () => {
    const userId = uid();
    await withInstaboxKey(userId);
    const create = instaboxApi("creating");
    const created = await cc("POST", userId, "/v1/cloud-computers", { provider: "instabox" });
    // Nothing for *Delete snapshots* to erase: InstaBox keeps no history, and a stopped disk needs no backup.
    expect(created.body.cloudComputer).toMatchObject({ provider: "instabox", providerName: "InstaBox", region: "US", state: "starting", snapshots: false });
    const [url, init] = create.mock.calls[0]!;
    expect(`${init!.method} ${url}`).toBe(`POST ${API}/v1/machines`);
    expect(JSON.parse(init!.body as string)).toMatchObject({ templateId: "agent-browser", vault: false, secrets: [] });
    const claimId = (init!.headers as Record<string, string>)["Idempotency-Key"]!;
    expect(claimId).toMatch(/^clm_/);
    expect((await row(userId))!.setup_at).toBeNull();

    // Still booting: nothing to set up yet.
    await cc("GET", userId);
    expect(calls(create).some((c) => c.endsWith("/exec"))).toBe(false);

    // Running and ready: one exec writes the ticket and starts the setup; later reads leave it be.
    const ready = instaboxApi("running");
    expect((await cc("GET", userId)).body.cloudComputers[0].state).toBe("running");
    expect(calls(ready)).toEqual([`GET ${API}/v1/machines/m_test`, `POST ${API}/v1/machines/m_test/exec`]);
    const ticket = instaboxTicket(ready);
    expect(ticket).toMatchObject({ userId, approval: "auto" });
    // A ticket of its own, made when it went out: the host has its whole life to install.
    expect(ticket.claimId).not.toBe(claimId);
    expect(ticket.claimId).toBe((await row(userId))!.claim_id);
    expect((await row(userId))!.setup_at).toEqual(expect.any(Number));
    await cc("GET", userId);
    expect(calls(ready).filter((c) => c.endsWith("/exec"))).toHaveLength(1);

    const host = await newHost();
    expect((await hostClaim(host, ticket)).status).toBe(200);
    const sleeping = instaboxApi("running");
    expect((await call("POST", "/v1/host/sleep", { key: host.sign, body: { wakeAt: null } })).status).toBe(200);
    const [sUrl, sInit] = sleeping.mock.calls.find(([, i]) => i?.method === "POST")!;
    expect(String(sUrl)).toBe(`${API}/v1/machines/m_test/suspend`);
    expect((sInit!.headers as Record<string, string>)["Idempotency-Key"]).toEqual(expect.any(String));

    const paused = instaboxApi("suspended");
    expect(await wake(env, host.cid, Date.now())).toBe("started");
    expect(calls(paused)).toEqual([`GET ${API}/v1/machines/m_test`, `POST ${API}/v1/machines/m_test/start`]);
    expect((paused.mock.calls[1]![1]!.headers as Record<string, string>)["Idempotency-Key"]).toEqual(expect.any(String));
    // Running but not ready yet: already starting, nothing to send.
    const booting = instaboxApi("running", {}, false);
    expect(await wake(env, host.cid, Date.now())).toBe("started");
    expect(calls(booting)).toEqual([`GET ${API}/v1/machines/m_test`]);

    const gone = instaboxApi("running");
    expect((await call("DELETE", `/v1/computers/${host.cid}`, { token: await clerkToken(userId) })).status).toBe(409);
    expect((await cc("DELETE", userId, `/v1/cloud-computers/${created.body.cloudComputer.cloudComputerId}`)).status).toBe(200);
    const [dUrl, dInit] = gone.mock.calls.find(([, i]) => i?.method === "DELETE")!;
    expect(String(dUrl)).toBe(`${API}/v1/machines/m_test`);
    expect(dInit!.headers).toMatchObject({ "X-Confirm-Delete": "m_test" });
  });

  it("is set up once though two apps read it at the same time, and again after a setup that didn't go out", async () => {
    const userId = await creating();
    const failing = instaboxApi("running", { "POST /v1/machines/m_test/exec": [503, { error: { code: "host_unavailable", message: "down", retryable: true } }] });
    await cc("GET", userId);
    expect(execs(failing)).toHaveLength(1);
    expect((await row(userId))!.setup_at).toBeNull();
    // InstaBox out of reach altogether: the read lists it as last seen, and the next one tries again.
    const unreachable = execUnreachable();
    expect((await cc("GET", userId)).status).toBe(200);
    expect(execs(unreachable)).toHaveLength(1);
    expect((await row(userId))!.setup_at).toBeNull();
    const both = instaboxApi("running");
    await Promise.all([cc("GET", userId), cc("GET", userId)]);
    expect(execs(both)).toHaveLength(1);
    expect((await row(userId))!.setup_at).toEqual(expect.any(Number));
  });

  it("is set up by the cron when no app reads it, one machine at a time", async () => {
    // Machines earlier tests left waiting are set up first, so this one is the only one left.
    instaboxApi("running");
    await setUpPending(env, Date.now());
    const userId = await creating();
    const unreachable = execUnreachable();
    await expect(setUpPending(env, Date.now())).resolves.toBeUndefined();
    expect(execs(unreachable)).toHaveLength(1);
    expect((await row(userId))!.setup_at).toBeNull();

    const cron = instaboxApi("running");
    await setUpPending(env, Date.now());
    expect(execs(cron)).toHaveLength(1);
    expect(await row(userId)).toMatchObject({ state: "running", setup_at: expect.any(Number) });
    const again = instaboxApi("running");
    await setUpPending(env, Date.now());
    expect(calls(again)).toEqual([]);
  });

  it("is started again when InstaBox paused it before its setup", async () => {
    const userId = await creating();
    const paused = instaboxApi("suspended");
    await cc("GET", userId);
    expect(calls(paused)).toEqual([`GET ${API}/v1/machines/m_test`, `POST ${API}/v1/machines/m_test/start`]);
    expect((await row(userId))!.setup_at).toBeNull();
    // The cron finds it too, though it was last seen asleep.
    const cron = instaboxApi("suspended");
    await setUpPending(env, Date.now());
    expect(calls(cron)).toContain(`POST ${API}/v1/machines/m_test/start`);
  });

  it("gets a ticket that is still good when it runs only after the first one expired", async () => {
    const userId = await creating();
    const first = (await env.DB.prepare("SELECT claim_id FROM cloud_machines WHERE user_id = ?").bind(userId).first<{ claim_id: string }>())!.claim_id;
    await env.DB.prepare("UPDATE claims SET expires_at = 0 WHERE id = ?").bind(first).run();
    const late = instaboxApi("running");
    await cc("GET", userId);
    const ticket = instaboxTicket(late);
    expect(ticket.claimId).not.toBe(first);
    const host = await newHost();
    expect((await hostClaim(host, ticket)).status).toBe(200);
    expect((await cc("GET", userId)).body.cloudComputers[0].computerId).toBe(host.cid);
  });
});

describe("a deleted account with a cloud computer on InstaBox", () => {
  it("forgets the key without pausing a computer InstaBox already paused", async () => {
    const userId = await creating();
    const fetch = instaboxApi("suspended");
    expect((await deliverClerk(`msg_${crypto.randomUUID()}`, { type: "user.deleted", data: { id: userId } })).status).toBe(200);
    expect(calls(fetch)).toEqual([`GET ${API}/v1/machines/m_test`]);
    expect(await env.DB.prepare("SELECT 1 FROM cloud_credentials WHERE user_id = ?").bind(userId).first()).toBeNull();
    expect((await row(userId))!.state).toBe("deleted");
  });
});
