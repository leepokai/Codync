// Cross-track integration test (spec §12.1): the real codync-host against `wrangler dev`, driven by a
// fake phone. Run with `npm run e2e` after `cargo build` in host/ (or set CODYNC_HOST_BIN).
//
// Steps: local D1 + wrangler dev with a test Clerk key and device certificate key → host with CODYNC_CLOUD_URL
// trusting that key → QR pairing over the relay with a certified phone, which the computer (in no account) lets in
// → chat through the relay → mailbox while the host is down → account claim + SAS access request → another
// account's phone turned away → cloud revocation → injected grant ignored → direct /channel and loopback-only
// bearer token.

import { type ChildProcess, spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { networkInterfaces } from "node:os";
import { join } from "node:path";
import * as ref from "../ref.ts";
import { check, freePort, LocalCloud, ROOT, step, stop } from "./harness.ts";
import { exerciseIcons } from "./icons.ts";
import { Channel, FakePhone, parsePairing, waitFor, type Json, type PairingInfo, type Wire } from "./phone.ts";

const HOST_BIN = process.env.CODYNC_HOST_BIN ?? join(ROOT, "host/target/debug/codync-host");
const FAKE_AGENT = join(ROOT, "host/tests/fake_agent.py");
const local = new LocalCloud(Number(process.env.CODYNC_E2E_CLOUD_PORT ?? (await freePort())));
const CLOUD = local.url;
const tmp = local.tmp;
const home = join(tmp, "home");
const run = local.run.bind(local);
const d1 = local.d1.bind(local);
const clerkToken = local.token.bind(local);
const cloud = local.call.bind(local);

// ---- the host ----

let hostPort = 0;
let host: ChildProcess | undefined;

async function startHost() {
  hostPort ||= await freePort();
  // 0.0.0.0 so step 7 can prove a non-loopback bearer token is refused.
  host = run(HOST_BIN, ["serve", "--bind", "0.0.0.0", "--port", String(hostPort)], {
    CODYNC_HOME: home,
    CODYNC_CLOUD_URL: CLOUD,
    CODYNC_DEVICE_CERT_KEY: local.certPub,
    CODYNC_AGENT_CHECKS: "off",
  });
  await waitFor("host /health", () => fetch(`http://127.0.0.1:${hostPort}/health`).then((r) => r.ok || undefined).catch(() => undefined), 30_000);
}

async function loopback(method: string, body: unknown = {}): Promise<Json> {
  const token = readFileSync(join(home, "token"), "utf8").trim();
  const res = await fetch(`http://127.0.0.1:${hostPort}/api/${method}`, {
    method: "POST",
    headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  const v = await res.json();
  if (!res.ok) throw new Error(`${method} → ${res.status} ${JSON.stringify(v)}`);
  return v;
}

/** A phone signed in to `token`'s account: registered with it, and the cloud's certificate for its key. */
async function certify(phone: FakePhone, token: string): Promise<Json> {
  check((await cloud("POST", "/v1/devices", { token, key: phone.key, body: { name: "E2E iPhone", platform: "ios" } })).status === 200, "phone registered with its account");
  const r = await cloud("POST", "/v1/devices/certificate", { token, key: phone.key });
  check(r.status === 200, "the cloud certified the phone's key");
  return r.body.certificate;
}

/** The relay refused the upgrade (the socket never opened). */
const refused = (wire: Wire) => wire.opened.then(
  () => false,
  () => true,
);

const computerOnline = async (id: string) => (await d1(`SELECT online FROM computers WHERE id = '${id}'`))[0]?.online === 1 || undefined;

// ---- scenario ----

async function main() {
  step("1. local D1 + wrangler dev");
  await local.start();

  step("2–3. host registers and comes online");
  await startHost();
  const health = await (await fetch(`http://127.0.0.1:${hostPort}/health`)).json();
  const computerId = health.computerId as string;
  check(/^[A-Za-z0-9_-]{22}$/.test(computerId), "host reports a computerId");
  await waitFor("computer row", async () => (await d1(`SELECT id FROM computers WHERE id = '${computerId}'`)).length > 0 || undefined);
  await waitFor("DO presence online", () => computerOnline(computerId), 20_000);
  check(true, "D1 row exists and the DO marked it online");

  step("4. QR pairing over the relay (the computer, in no account, asks first), then chat");
  const userId = "user_e2e";
  const token = await clerkToken(userId);
  const pairing: PairingInfo = parsePairing((await loopback("pairing")).pairingUrl);
  check(pairing.computerId === computerId && pairing.cloud === CLOUD, "pairing link names this computer and cloud");
  const phone = new FakePhone();
  // No claim: the computer stays out of the account until step 6 claims it.
  const pairs = phone.pair(pairing, { certificate: await certify(phone, token) });
  const ask = await waitFor("the computer asks", async () => (await loopback("pairingRequests")).requests[0]);
  check(ask.email === `${userId}@example.com` && ask.code === ref.pairCheck(phone.key.pub, pairing.signKey), `it shows the phone's account and code (${ask.code})`);
  await loopback("decidePairing", { requestId: ask.requestId, approve: true });
  const paired = await pairs;
  check(paired.computerId === computerId && paired.claim === null, "pair answered, link closed with 4100");
  // 6c: the normal reconnect is not a pairing socket, so it isn't capped at 16 messages.
  let ch = await phone.relay(CLOUD, computerId, pairing.signKey);
  const hello = await ch.call("hello", {});
  check(hello.computerId === computerId && hello.boxKey === ref.b64url(pairing.boxKey), "inner hello returns computerId and boxKey");
  for (let i = 0; i < 20; i++) await ch.call("hello", {});
  check(true, "more than 16 messages on the regular link");

  if (process.env.CODYNC_E2E_ICONS_ONLY === "1") {
    // The phone's own account: joining another one would turn the phone away.
    const claim = await cloud("POST", "/v1/claims", { token });
    const signed = await loopback("claimSign", { claimId: claim.body.claimId, nonce: claim.body.nonce, userId });
    check((await cloud("POST", `/v1/claims/${claim.body.claimId}/complete`, { token, body: signed })).status === 200, "icon test computer claimed");
    await waitFor("icon host sees its owner", async () => (await loopback("cloudStatus")).owner?.userId === userId || undefined);
    const target = `bot:${(await ch.call("createBot", { name: "Icons", backend: "custom", command: "true", cwd: tmp })).bot.id}`;
    const png = await exerciseIcons((method, body) => ch.call(method, body, 60_000), target);
    check(await ch.call("setIcon", { target: "agent:claude", remove: true }).then(() => false, () => true), "agents have no custom icons");
    await stop(host!);
    await startHost();
    check((await loopback("hello")).icons[target].dataUrl === png, "icon remains cached after host restart");
    return;
  }

  const bot = (await ch.call("createBot", { name: "Tester", backend: "custom", command: `python3 '${FAKE_AGENT}'`, cwd: tmp, permission: "ask" })).bot.id;
  ch.subscribe("events", { since: 0, client: "ios" });
  await ch.call("send", { botId: bot, text: "go", clientNonce: "e2e-1" });
  const card = await waitFor("permission card", () =>
    ch.events.find((e) => e.type === "entry" && e.entry.kind === "permission" && e.entry.data.status === "pending"),
  );
  await ch.call("respondPermission", { entryId: card.entry.id, optionId: "allow" });
  await waitFor("final reply", () => ch.events.find((e) => e.type === "entry" && e.entry.kind === "agent" && e.entry.data.final === true), 20_000);
  check(true, "agent reply arrived through the relay");

  step("5. mailbox while the host is down");
  await stop(host!);
  await ch.wire.next((m) => m.t === "presence" && m.online === false, "presence offline");
  check(true, "device saw presence online:false");
  phone.mailboxPut(ch.wire, pairing, bot, "queued hello", "e2e-mbox-1");
  check((await ch.wire.next((m) => m.t === "mbox.ok", "mbox.ok")).nonce === "e2e-mbox-1", "queued");
  phone.mailboxPut(ch.wire, pairing, bot, "never mind", "e2e-mbox-2");
  await ch.wire.next((m) => m.t === "mbox.ok", "mbox.ok 2");
  ch.wire.send({ t: "mbox.cancel", nonce: "e2e-mbox-2" });
  check((await ch.wire.next((m) => m.t === "mbox.cancelled", "cancelled")).nonce === "e2e-mbox-2", "second one cancelled");
  await startHost();
  const delivered = await ch.wire.next((m) => m.t === "mbox.delivered", "mbox.delivered", 30_000);
  check(delivered.nonce === "e2e-mbox-1", "delivered after the host came back");
  // Every presence online:true means a fresh handshake on the same socket (§7.2).
  ch = await Channel.open(ch.wire, phone.key, pairing.signKey, { relay: true });
  const history = await waitFor("mailbox entry", async () => {
    const h = await ch.call("history", { botId: bot });
    return h.entries.some((e: Json) => e.data?.clientNonce === "e2e-mbox-1") ? h : undefined;
  });
  check(!history.entries.some((e: Json) => e.data?.clientNonce === "e2e-mbox-2"), "the cancelled message never ran");

  step("5b. public routine webhooks through the cloud");
  const hookRoutine = async (name: string) =>
    (await loopback("saveRoutine", { botId: bot, name, instruction: "Summarize the delivery", triggers: [{ type: "webhook" }] })).routine.id as string;
  const runFor = (routine: string, delivery: string) => async () =>
    ((await loopback("routines", { botId: bot })).runs as Json[]).find((r) => r.routineId === routine && r.deliveryId === delivery);
  const live = await hookRoutine("Live hook");
  const creds = await loopback("routineWebhook", { botId: bot, id: live });
  check(creds.url === `${CLOUD}/v1/hooks/${computerId}/${live}`, "public URL names this computer and routine");
  // The host registers its hooks right after it comes online; give the set a moment to land.
  const deliver = (url: string, headers: Record<string, string>, body: string) =>
    fetch(url, { method: "POST", headers: { "Content-Type": "application/json", ...headers }, body });
  await waitFor("hook registered", async () => (await deliver(creds.url, { Authorization: "Bearer nope" }, "{}")).status === 401 || undefined);
  const accepted = await waitFor("bearer delivery accepted", async () => {
    const res = await deliver(creds.url, { Authorization: `Bearer ${creds.key}`, "X-Delivery-Id": "e2e-hook-1" }, '{"text":"build failed"}');
    return res.status === 202 ? res : undefined;
  });
  check(accepted.status === 202, "bearer delivery queued (202)");
  const run1 = await waitFor("run from the public delivery", runFor(live, "e2e-hook-1"), 20_000);
  check(run1.event?.text === "build failed", "the run carries the delivered event");
  check((await deliver(creds.url, { Authorization: "Bearer wrong" }, "{}")).status === 401, "a wrong key is refused at the edge");

  const offline = await hookRoutine("Offline hook");
  const offCreds = await loopback("routineWebhook", { botId: bot, id: offline });
  await waitFor("second hook registered", async () =>
    (await deliver(offCreds.url, { Authorization: "Bearer nope" }, "{}")).status === 401 || undefined,
  );
  await stop(host!);
  const ghBody = '{"action":"opened","pull_request":{"title":"Add hooks"},"repository":{"full_name":"o/r"}}';
  const mac = await crypto.subtle.sign(
    "HMAC",
    await crypto.subtle.importKey("raw", ref.enc.encode(offCreds.key), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]),
    ref.enc.encode(ghBody),
  );
  const sig = `sha256=${Buffer.from(mac).toString("hex")}`;
  const queued = await deliver(offCreds.url, { "X-GitHub-Event": "pull_request", "X-GitHub-Delivery": "e2e-gh-1", "X-Hub-Signature-256": sig }, ghBody);
  check(queued.status === 202, "a signed GitHub delivery is queued while the host is down");
  await startHost();
  const run2 = await waitFor("run after the host came back", runFor(offline, "e2e-gh-1"), 30_000);
  check(run2.event?.event === "pull_request.opened" && run2.event?.repo === "o/r", "delivered once the host is back, as a GitHub event");
  ch = await Channel.open(ch.wire, phone.key, pairing.signKey, { relay: true });

  step("6. account: claim, access request with SAS, another account's phone, revocation");
  const claim = await cloud("POST", "/v1/claims", { token });
  const signed = await loopback("claimSign", { claimId: claim.body.claimId, nonce: claim.body.nonce, userId });
  const done = await cloud("POST", `/v1/claims/${claim.body.claimId}/complete`, { token, body: signed });
  check(done.status === 200 && done.body.computer.computerId === computerId, "computer claimed");
  await waitFor("host sees its owner", async () => (await loopback("cloudStatus")).owner?.userId === userId || undefined);
  check((await loopback("devices")).devices.some((d: Json) => d.key === phone.dk), "the phone paired before the claim keeps its access");

  const stranger = new FakePhone();
  const strangerPairing = parsePairing((await loopback("pairing")).pairingUrl);
  const turnedAway = await stranger.pair(strangerPairing, { certificate: await certify(stranger, await clerkToken("someone_else")) }).catch((e: Error) => e);
  check(turnedAway instanceof Error && turnedAway.message.includes("u•••@example.com's account"), "a phone of another account is turned away, the owner masked");

  const phone2 = new FakePhone();
  check((await cloud("POST", "/v1/devices", { token, key: phone2.key, body: { name: "Phone 2", platform: "ios" } })).status === 200, "phone 2 registered");
  const listed = (await cloud("GET", "/v1/computers", { token, key: phone2.key })).body.computers[0];
  check(listed.access === "none", "phone 2 has no access yet");
  const nD = ref.random(32);
  const req = await cloud("POST", `/v1/computers/${computerId}/access-requests`, {
    token,
    key: phone2.key,
    body: { commit: ref.b64url(ref.sasCommit(phone2.key.pub, nD)) },
  });
  const requestId = req.body.requestId as string;
  const hostNonce = await waitFor("host nonce", async () => (await cloud("GET", `/v1/access-requests/${requestId}`, { token })).body.hostNonce);
  check((await cloud("POST", `/v1/access-requests/${requestId}/reveal`, { token, key: phone2.key, body: { nonce: ref.b64url(nD) } })).status === 200, "revealed");
  const sas = ref.sasCode(ref.unb64url(listed.signKey), phone2.key.pub, nD, ref.unb64url(hostNonce));
  const shown = await waitFor("SAS on the computer", async () =>
    (await loopback("accessRequests")).requests.find((r: Json) => r.requestId === requestId && r.code),
  );
  check(shown.code === sas, `the computer shows the phone's SAS (${sas})`);
  await loopback("decideAccessRequest", { requestId, approve: true });
  const approved = await waitFor("approved", async () => {
    const r = (await cloud("GET", `/v1/access-requests/${requestId}`, { token })).body;
    return r.status === "approved" ? r : undefined;
  });
  const ch2 = await phone2.relay(CLOUD, computerId, ref.unb64url(listed.signKey));
  check((await ch2.call("hello", {})).computerId === computerId, "phone 2 connected through the relay");

  const cachedIcon = await exerciseIcons((method, body) => ch2.call(method, body, 60_000), `bot:${bot}`);

  const started = Date.now();
  await cloud("DELETE", `/v1/computers/${computerId}/grants/${approved.grantId}`, { token });
  const code = await ch2.wire.closed(5000);
  check(code === 4003 && Date.now() - started < 1000, `revoked socket closed with 4003 within 1 s (${Date.now() - started} ms)`);
  check(await refused(phone2.relayWire(CLOUD, computerId)), "phone 2 can't reconnect");
  await stop(host!);
  await startHost();
  await waitFor("host back online", () => computerOnline(computerId), 20_000);
  check(await refused(phone2.relayWire(CLOUD, computerId)), "still refused after the host reconnects");
  check((await loopback("hello")).icons[`bot:${bot}`].dataUrl === cachedIcon, "icon survives a real host restart in its local cache");

  step("6b. a grant injected into D1 is ignored by the host");
  const phone3 = new FakePhone();
  await cloud("POST", "/v1/devices", { token, key: phone3.key, body: { name: "Intruder", platform: "ios" } });
  const r3 = await cloud("POST", `/v1/computers/${computerId}/access-requests`, { token, key: phone3.key, body: { commit: ref.b64url(ref.random(32)) } });
  const dev3 = (await d1(`SELECT id FROM devices WHERE sign_pub = '${phone3.dk}'`))[0].id;
  await d1(`INSERT INTO grants(id, computer_id, device_id, scopes, status, created_at) VALUES ('grt_injected', '${computerId}', '${dev3}', '["control","screen"]', 'active', ${Date.now()})`);
  await cloud("DELETE", `/v1/access-requests/${r3.body.requestId}`, { token }); // → cloud.changed → state pull
  await new Promise((r) => setTimeout(r, 2000));
  const devices = (await loopback("devices")).devices as Json[];
  check(!devices.some((d) => d.key === phone3.dk), "host's device table doesn't contain the injected key");
  check(await refused(phone3.relayWire(CLOUD, computerId)), "the injected key can't open a relay socket (not in the ACL)");

  step("7. direct /channel and loopback-only bearer token");
  const direct = await phone.direct(`http://127.0.0.1:${hostPort}`, pairing.signKey);
  check((await direct.call("hello", {})).computerId === computerId, "direct handshake and hello");
  const lan = Object.values(networkInterfaces())
    .flat()
    .find((i) => i && i.family === "IPv4" && !i.internal)?.address;
  if (lan) {
    const token = readFileSync(join(home, "token"), "utf8").trim();
    const res = await fetch(`http://${lan}:${hostPort}/api/hello`, { method: "POST", headers: { Authorization: `Bearer ${token}` }, body: "{}" });
    check(res.status === 401, `bearer token from ${lan} → 401`);
  } else {
    console.log("  (skipped: no non-loopback IPv4 interface)");
  }

  console.log("\nE2E passed");
}

const cleanup = () => local.cleanup();

try {
  if (spawnSync(HOST_BIN, ["--version"]).error) {
    throw new Error(`codync-host not found at ${HOST_BIN}; run \`cargo build\` in host/ or set CODYNC_HOST_BIN`);
  }
  await main();
  await cleanup();
} catch (e) {
  console.error(`\nE2E failed: ${e instanceof Error ? e.stack : e}`);
  await cleanup();
  process.exit(1);
}
