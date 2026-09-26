// ComputerRelay DO through the Worker: admission, ACL, presence, forwarding, mailbox, limits.

import { runDurableObjectAlarm, runInDurableObject, SELF } from "cloudflare:test";
import { describe, expect, it } from "vitest";
import V from "../../docs/remote-relay-vectors.json";
import type { ComputerRelay } from "../src/relay";
import { acl, call, devicePath, env, hostSocket, newHost, sleep, socket, upgrade, type TestHost } from "./helpers";
import * as ref from "./ref";

const stub = (host: TestHost) => env.RELAY.get(env.RELAY.idFromName(host.cid)) as DurableObjectStub<ComputerRelay>;
const dkOf = (k: ref.SignKey) => ref.b64url(k.pub);
const blob = (host: TestHost, device: ref.SignKey, nonce: string) =>
  ref.b64url(
    ref.sealMailbox(device, host.sign.pub, host.box.pub, nonce, JSON.stringify({ m: "send", b: { botId: "b1", text: "hi", clientNonce: nonce }, ts: Date.now() })),
  );

/** A host with one authorized (local) device, both connected; the host is ready unless told otherwise. */
async function pair(o: { ready?: boolean } = {}) {
  const host = await newHost();
  const device = ref.signKey();
  const hostWs = await hostSocket(host, [{ dk: dkOf(device), grant: null }], o);
  const dev = await socket(devicePath(host), device);
  return { host, device, hostWs, dev };
}

describe("routing", () => {
  it("validates version and computerId before touching a DO", async () => {
    const key = ref.signKey();
    const host = await newHost();
    expect((await upgrade(`/v1/relay/device/${host.cid}?v=2`, key)).status).toBe(426);
    expect((await upgrade(`/v1/relay/device/${host.cid}`, key)).body.error.code).toBe("upgradeRequired");
    expect((await upgrade("/v1/relay/device/short?v=1", key)).status).toBe(400);
    expect((await upgrade(`/v1/relay/device/${host.cid}!?v=1`, key)).status).toBe(400);
    expect((await upgrade(`/v1/relay/device/${host.cid}?v=1&pair=nope`, key)).status).toBe(400);
  });

  it("keeps /internal/* unreachable from outside", async () => {
    for (const path of ["/internal/block", "/v1/relay/internal/block", "/v1/relay/device/%2Finternal%2Fblock?v=1", "/relay"]) {
      const res = await SELF.fetch(`https://cloud.test${path}`, { method: "POST", headers: { "X-Codync-Internal": "1" }, body: "{}" });
      expect(res.status, path).not.toBe(200);
    }
    const host = await newHost();
    // The DO itself refuses anything without the Worker's internal header.
    const res = await stub(host).fetch("https://do/internal/block", { method: "POST", body: JSON.stringify({ devices: [] }) });
    expect(res.status).toBe(404);
    expect((await stub(host).fetch("https://do/other", { headers: { "X-Codync-Internal": "1" } })).status).toBe(404);
  });

  it("requires registration for the host and a known computer for devices", async () => {
    const stranger = ref.signKey();
    const r = await upgrade("/v1/relay/host?v=1", stranger);
    expect(r.status).toBe(404);
    expect((await upgrade(`/v1/relay/device/${ref.computerId(stranger.pub)}?v=1`, ref.signKey())).status).toBe(404);
    // Registered but never connected: no ACL yet.
    const host = await newHost();
    const d = await upgrade(devicePath(host), ref.signKey());
    expect(d.status).toBe(404);
    expect(d.body.error.code).toBe("unknownComputer");
  });

  it("rejects a replayed upgrade signature", async () => {
    const host = await newHost();
    const nonce = ref.b64url(ref.random(16));
    const first = await upgrade("/v1/relay/host?v=1", host.sign, { nonce });
    expect(first.status).toBe(101);
    expect((await upgrade("/v1/relay/host?v=1", host.sign, { nonce })).status).toBe(401);
    expect((await upgrade("/v1/relay/host?v=1", host.sign, { authority: "other.example" })).status).toBe(401);
  });
});

describe("ACL", () => {
  it("accepts the vector ACL and enforces monotonic versions", async () => {
    const host = await newHost(ref.signKey(ref.unb64url(V.keys.hostSignSeed)), ref.boxKey(ref.unb64url(V.keys.hostBoxPriv)));
    expect(host.cid).toBe(V.keys.computerId);
    const ws = await socket("/v1/relay/host?v=1", host.sign);
    ws.send({ t: "acl", d: V.acl.d, sig: V.acl.sig });
    expect(await ws.nextT("acl.ok")).toEqual({ t: "acl.ok", ver: 1 });
    ws.send({ t: "acl", d: V.acl.d, sig: V.acl.sig });
    expect(await ws.next()).toEqual({ t: "error", code: "staleVersion", ver: 1 });
    ws.send({ t: "acl", d: V.acl.d, sig: ref.b64url(ref.random(64)) });
    expect(await ws.next()).toEqual({ t: "error", code: "badAcl", ver: 1 });
    // Signed by the host but for another computer.
    ws.send(ref.aclMessage(host.sign, { v: 1, computerId: "A".repeat(22), ver: 5, devices: [], offers: [] }));
    expect(await ws.next()).toEqual({ t: "error", code: "badAcl", ver: 1 });
    // Signed by someone else.
    ws.send(ref.aclMessage(ref.signKey(), { v: 1, computerId: host.cid, ver: 5, devices: [], offers: [] }));
    expect(await ws.next()).toEqual({ t: "error", code: "badAcl", ver: 1 });
    // The vector's device (local, grant null) is admitted; its offer is expired (exp in the past).
    expect((await upgrade(devicePath(host), ref.signKey(ref.unb64url(V.keys.deviceSignSeed)))).status).toBe(101);
    expect((await upgrade(devicePath(host, V.pairing.offerId), ref.signKey())).status).toBe(403);
  });

  it("closes sockets the new ACL drops", async () => {
    const { host, device, hostWs, dev } = await pair();
    await dev.nextT("presence");
    hostWs.send(acl(host, []));
    await hostWs.nextT("acl.ok");
    expect(await dev.closed()).toBe(4003);
    expect((await upgrade(devicePath(host), device)).status).toBe(403);
  });
});

describe("admission", () => {
  it("admits only devices in the ACL", async () => {
    const { host } = await pair();
    expect((await upgrade(devicePath(host), ref.signKey())).status).toBe(403);
  });

  it("limits pairing sockets: live offer, two at a time, 16 messages, no mailbox, closed when the offer goes", async () => {
    const host = await newHost();
    const offer = ref.offerId(ref.random(16));
    const hostWs = await hostSocket(host, [], { offers: [{ id: offer, exp: Date.now() + 600_000 }] });
    expect((await upgrade(devicePath(host, ref.offerId(ref.random(16))), ref.signKey())).status).toBe(403);
    const a = await socket(devicePath(host, offer), ref.signKey());
    const b = await socket(devicePath(host, offer), ref.signKey());
    expect((await upgrade(devicePath(host, offer), ref.signKey())).status).toBe(429);
    const open = await hostWs.nextT("open");
    expect(open.pair).toBe(true);

    a.send({ t: "mbox.put", nonce: "n", d: "AAAA" });
    expect(await a.nextT("mbox.err")).toEqual({ t: "mbox.err", nonce: "n", code: "pairing" });
    for (let i = 0; i < 16; i++) a.send({ t: "mbox.list" });
    expect(await a.closed()).toBe(4008);

    hostWs.send(acl(host, [], []));
    await hostWs.nextT("acl.ok");
    expect(await b.closed()).toBe(4410);
  });

  it("caps device sockets per computer at 16", async () => {
    const host = await newHost();
    const keys = Array.from({ length: 17 }, () => ref.signKey());
    await hostSocket(host, keys.map((k) => ({ dk: dkOf(k), grant: null })));
    for (const k of keys.slice(0, 16)) expect((await upgrade(devicePath(host), k)).status).toBe(101);
    expect((await upgrade(devicePath(host), keys[16]!)).status).toBe(429);
  });
});

describe("presence and forwarding", () => {
  it("before ready: devices see offline, channel messages are dropped, mailbox is open", async () => {
    const { host, device, hostWs, dev } = await pair({ ready: false });
    expect(await dev.nextT("presence")).toMatchObject({ online: false });
    dev.send({ t: "hello", v: 1 });
    expect(await dev.nextT("presence")).toMatchObject({ online: false });
    await hostWs.none((m) => m.t === "data" || m.t === "open");
    dev.send({ t: "mbox.put", nonce: "n1", d: blob(host, device, "n1") });
    expect(await dev.nextT("mbox.ok")).toEqual({ t: "mbox.ok", nonce: "n1", state: "queued" });

    hostWs.send({ t: "ready" });
    expect(await dev.nextT("presence")).toMatchObject({ online: true });
    const open = await hostWs.nextT("open");
    expect(open).toMatchObject({ dk: dkOf(device), pair: false });
    expect(await hostWs.nextT("mbox.item")).toMatchObject({ from: dkOf(device), nonce: "n1" });
    // After ready a new device socket is announced at once.
    const late = await socket(devicePath(host), device);
    expect(await late.nextT("presence")).toMatchObject({ online: true });
    expect(await hostWs.nextT("open")).toMatchObject({ dk: dkOf(device) });
    // And the mailbox is closed while the host is online.
    late.send({ t: "mbox.put", nonce: "n2", d: blob(host, device, "n2") });
    expect(await late.nextT("mbox.err")).toMatchObject({ code: "hostOnline" });
    const row = await env.DB.prepare("SELECT online, last_seen_at FROM computers WHERE id = ?").bind(host.cid).first<{ online: number }>();
    expect(row?.online).toBe(1);
  });

  it("carries a full E2E handshake and frames without reading them", async () => {
    const { host, device, hostWs, dev } = await pair();
    const { link } = await hostWs.nextT("open");
    await dev.nextT("presence");
    const d = ref.deviceHello(device, host.sign.pub);
    dev.send(d.hello);
    const data = await hostWs.nextT("data");
    expect(data.link).toBe(link);
    expect(data.m).toEqual(d.hello);
    const h = ref.hostWelcome(host.sign, data.m);
    hostWs.send({ t: "data", link, m: h.welcome });
    const keys = d.finish(await dev.nextT("welcome"));
    const phone = new ref.FrameCodec(keys.d2h, keys.h2d);
    const computer = new ref.FrameCodec(h.keys.h2d, h.keys.d2h);
    for (const f of phone.seal({ id: 1, m: "hello", b: {} })) dev.send(f);
    expect(computer.open((await hostWs.nextT("data")).m)).toEqual({ id: 1, m: "hello", b: {} });
    for (const f of computer.seal({ id: 1, ok: { name: "Mac" } })) hostWs.send({ t: "data", link, m: f });
    expect(phone.open(await dev.nextT("f"))).toEqual({ id: 1, ok: { name: "Mac" } });

    // The host closes a link with a code the device sees (4100 after pairing).
    hostWs.send({ t: "close", link, code: 4100, reason: "paired" });
    expect(await dev.closed()).toBe(4100);
    expect(await hostWs.nextT("close")).toEqual({ t: "close", link });
  });

  it("answers the exact ping string by auto-response", async () => {
    const { hostWs, dev } = await pair();
    dev.send('{"t":"ping"}');
    expect(await dev.nextT("pong")).toEqual({ t: "pong" });
    hostWs.send('{"t":"ping"}');
    expect(await hostWs.nextT("pong")).toEqual({ t: "pong" });
  });

  it("a new host socket replaces the old: 4009, then offline → online", async () => {
    const { host, device, hostWs, dev } = await pair();
    expect(await dev.nextT("presence")).toMatchObject({ online: true });
    const next = await socket("/v1/relay/host?v=1", host.sign);
    expect(await hostWs.closed()).toBe(4009);
    expect(await dev.nextT("presence")).toMatchObject({ online: false });
    next.send(acl(host, [{ dk: dkOf(device), grant: null }]));
    await next.nextT("acl.ok");
    next.send({ t: "ready" });
    expect(await dev.nextT("presence")).toMatchObject({ online: true });
    expect(await next.nextT("open")).toBeTruthy();
  });

  it("marks the host offline when its socket closes or goes silent", async () => {
    const { host, hostWs, dev } = await pair();
    await dev.nextT("presence");
    hostWs.ws.close(1000, "bye");
    const off = await dev.nextT("presence");
    expect(off.online).toBe(false);
    expect(typeof off.lastSeenAt).toBe("number");
    const row = await env.DB.prepare("SELECT online, last_seen_at FROM computers WHERE id = ?").bind(host.cid).first<{ online: number; last_seen_at: number }>();
    expect(row?.online).toBe(0);
    expect(row?.last_seen_at).toBe(off.lastSeenAt);

    const again = await hostSocket(host, [{ dk: dkOf(ref.signKey()), grant: null }]);
    expect(await dev.closed()).toBe(4003); // not in the new ACL
    // Silent for > 90 s: the alarm closes it with 1011.
    await runInDurableObject(stub(host), (_, state) => {
      for (const ws of state.getWebSockets("host")) ws.serializeAttachment({ ...ws.deserializeAttachment(), at: Date.now() - 91_000 });
    });
    await runDurableObjectAlarm(stub(host));
    expect(await again.closed()).toBe(1011);
  });
});

describe("mailbox", () => {
  it("is idempotent, cancellable, listable and validated", async () => {
    const { host, device, hostWs, dev } = await pair({ ready: false });
    await dev.nextT("presence");
    const put = (nonce: string, extra: object = {}) => dev.send({ t: "mbox.put", nonce, d: blob(host, device, nonce), ...extra });
    put("a");
    expect(await dev.nextT("mbox.ok")).toEqual({ t: "mbox.ok", nonce: "a", state: "queued" });
    put("a");
    expect(await dev.nextT("mbox.ok")).toEqual({ t: "mbox.ok", nonce: "a", state: "queued" });
    put("b", { exp: Date.now() + 60_000 });
    await dev.nextT("mbox.ok");
    dev.send({ t: "mbox.list" });
    const list = await dev.nextT("mbox.items");
    expect(list.items.map((i: { nonce: string; state: string }) => [i.nonce, i.state])).toEqual([["a", "queued"], ["b", "queued"]]);

    dev.send({ t: "mbox.cancel", nonce: "b" });
    expect(await dev.nextT("mbox.cancelled")).toEqual({ t: "mbox.cancelled", nonce: "b" });
    dev.send({ t: "mbox.cancel", nonce: "zzz" });
    expect(await dev.nextT("mbox.gone")).toEqual({ t: "mbox.gone", nonce: "zzz", state: "unknown" });

    put("late", { exp: Date.now() + 25 * 3600_000 });
    expect(await dev.nextT("mbox.err")).toMatchObject({ nonce: "late", code: "invalid" });
    dev.send({ t: "mbox.put", nonce: "junk", d: "AAAA" });
    expect(await dev.nextT("mbox.err")).toMatchObject({ code: "invalid" });
    dev.send({ t: "mbox.put", nonce: "big", d: ref.b64url(new Uint8Array(64 * 1024 + 1).fill(7)) });
    expect(await dev.nextT("mbox.err")).toMatchObject({ nonce: "big", code: "tooLarge" });

    // Delivery: one at a time in seq order; a delivering item can't be cancelled.
    hostWs.send({ t: "ready" });
    const item = await hostWs.nextT("mbox.item");
    expect(item).toMatchObject({ from: dkOf(device), nonce: "a" });
    expect(ref.openMailbox(host.sign.pub, host.box, device.pub, "a", ref.unb64url(item.d))).toContain('"clientNonce":"a"');
    dev.send({ t: "mbox.cancel", nonce: "a" });
    expect(await dev.nextT("mbox.gone")).toEqual({ t: "mbox.gone", nonce: "a", state: "delivering" });
    hostWs.send({ t: "mbox.ack", seq: item.seq, ok: true });
    expect(await dev.nextT("mbox.delivered")).toEqual({ t: "mbox.delivered", nonce: "a" });
  });

  it("delivers in order, requeues on host disconnect, and reports failures", async () => {
    const { host, device, hostWs, dev } = await pair({ ready: false });
    await dev.nextT("presence");
    for (const n of ["1", "2", "3"]) {
      dev.send({ t: "mbox.put", nonce: n, d: blob(host, device, n) });
      await dev.nextT("mbox.ok");
    }
    hostWs.send({ t: "ready" });
    const first = await hostWs.nextT("mbox.item");
    expect(first.nonce).toBe("1");
    await hostWs.none((m) => m.t === "mbox.item");
    hostWs.send({ t: "mbox.ack", seq: first.seq, ok: false, code: "unknownBot" });
    expect(await dev.nextT("mbox.failed")).toEqual({ t: "mbox.failed", nonce: "1", code: "unknownBot" });
    const second = await hostWs.nextT("mbox.item");
    expect(second.nonce).toBe("2");
    // Host drops before acking: "2" goes back to the queue and comes again, same seq.
    hostWs.ws.close(1000);
    await dev.nextT("presence");
    const back = await hostSocket(host, [{ dk: dkOf(device), grant: null }]);
    const again = await back.nextT("mbox.item");
    expect(again).toMatchObject({ seq: second.seq, nonce: "2" });
    back.send({ t: "mbox.ack", seq: again.seq, ok: true });
    expect((await back.nextT("mbox.item")).nonce).toBe("3");
  });

  it("enforces the per-device limit and expires items", async () => {
    const { host, device, dev } = await pair({ ready: false });
    await dev.nextT("presence");
    const tiny = ref.b64url(ref.random(128));
    for (let i = 0; i < 50; i++) dev.send({ t: "mbox.put", nonce: `n${i}`, d: tiny });
    for (let i = 0; i < 50; i++) await dev.nextT("mbox.ok");
    dev.send({ t: "mbox.put", nonce: "n50", d: tiny });
    expect(await dev.nextT("mbox.err")).toEqual({ t: "mbox.err", nonce: "n50", code: "full" });

    await runInDurableObject(stub(host), (_, state) => {
      state.storage.sql.exec("UPDATE mailbox SET exp = ? WHERE nonce = 'n0'", Date.now() - 1);
    });
    await runDurableObjectAlarm(stub(host));
    expect(await dev.nextT("mbox.expired")).toEqual({ t: "mbox.expired", nonce: "n0" });
    dev.send({ t: "mbox.put", nonce: "n50", d: blob(host, device, "n50") });
    expect(await dev.nextT("mbox.ok")).toMatchObject({ nonce: "n50" });
  });

  it("accepts mail for a host that has been offline for hours (the DO ignores leases)", async () => {
    const host = await newHost();
    const device = ref.signKey();
    const hostWs = await hostSocket(host, [{ dk: dkOf(device), grant: "grt_account" }]);
    hostWs.ws.close(1000);
    await sleep(50);
    await runInDurableObject(stub(host), (_, state) => {
      state.storage.sql.exec("UPDATE meta SET v = ? WHERE k = 'lastSeenAt'", String(Date.now() - 3 * 3600_000));
    });
    const dev = await socket(devicePath(host), device);
    expect(await dev.nextT("presence")).toMatchObject({ online: false });
    dev.send({ t: "mbox.put", nonce: "later", d: blob(host, device, "later") });
    expect(await dev.nextT("mbox.ok")).toMatchObject({ state: "queued" });
  });
});

describe("rate limits", () => {
  it("closes a device socket past 200 messages in 10 s", async () => {
    const { dev } = await pair({ ready: false });
    for (let i = 0; i < 201; i++) dev.send({ t: "mbox.list" });
    expect(await dev.closed()).toBe(4008);
  });

  it("closes on malformed or unknown messages", async () => {
    const { dev } = await pair();
    dev.send("not json");
    expect(await dev.closed()).toBe(4002);
  });
});

describe("host health check via the API", () => {
  it("registration reports ownership", async () => {
    const key = ref.signKey();
    const r = await call("POST", "/v1/host/register", {
      key,
      body: { boxKey: ref.b64url(ref.boxKey().pub), name: "Mac", platform: "macos", version: "2.2.0" },
    });
    expect(r.body).toEqual({ computerId: ref.computerId(key.pub), owned: false });
  });
});
