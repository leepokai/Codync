// Cross-track integration test (spec §12.1): the real codync-host against `wrangler dev`, driven by a
// fake phone. Run with `npm run e2e` after `cargo build` in host/ (or set CODYNC_HOST_BIN).
//
// Steps: local D1 + wrangler dev with a test Clerk key → host with CODYNC_CLOUD_URL → QR pairing over the
// relay → chat through the relay → mailbox while the host is down → account claim + SAS access request →
// cloud revocation → injected grant ignored → direct /channel and loopback-only bearer token.

import { type ChildProcess, spawn, spawnSync } from "node:child_process";
import { generateKeyPairSync } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { networkInterfaces, tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import * as ref from "../ref.ts";
import { Channel, FakePhone, parsePairing, waitFor, type Json, type PairingInfo, type Wire } from "./phone.ts";

const CLOUD_DIR = fileURLToPath(new URL("../..", import.meta.url));
const ROOT = join(CLOUD_DIR, "..");
const HOST_BIN = process.env.CODYNC_HOST_BIN ?? join(ROOT, "host/target/debug/codync-host");
const FAKE_AGENT = join(ROOT, "host/tests/fake_agent.py");
const CLOUD_PORT = Number(process.env.CODYNC_E2E_CLOUD_PORT ?? 8787);
const CLOUD = `http://127.0.0.1:${CLOUD_PORT}`;
const ISSUER = "https://clerk.test.local"; // [env.dev] CLERK_ISSUER
const WRANGLER = join(CLOUD_DIR, "node_modules/.bin/wrangler");

const tmp = mkdtempSync(join(tmpdir(), "codync-e2e-"));
const persist = join(tmp, "wrangler");
const home = join(tmp, "home");
const children: ChildProcess[] = [];

function step(name: string) {
  console.log(`\n▶ ${name}`);
}

function check(cond: unknown, what: string): asserts cond {
  if (!cond) throw new Error(`check failed: ${what}`);
  console.log(`  ✓ ${what}`);
}

async function freePort(): Promise<number> {
  return new Promise((resolve) => {
    const s = createServer();
    s.listen(0, "127.0.0.1", () => {
      const port = (s.address() as { port: number }).port;
      s.close(() => resolve(port));
    });
  });
}

function run(cmd: string, args: string[], env: Record<string, string> = {}): ChildProcess {
  const child = spawn(cmd, args, { cwd: CLOUD_DIR, env: { ...process.env, ...env }, stdio: ["ignore", "pipe", "pipe"] });
  const tag = cmd.split("/").pop();
  child.stdout?.on("data", (d) => process.env.E2E_VERBOSE && process.stdout.write(`[${tag}] ${d}`));
  child.stderr?.on("data", (d) => process.env.E2E_VERBOSE && process.stderr.write(`[${tag}] ${d}`));
  children.push(child);
  return child;
}

async function stop(child: ChildProcess) {
  if (child.exitCode !== null || child.signalCode) return;
  const exited = new Promise((r) => child.once("exit", r));
  child.kill("SIGTERM");
  await Promise.race([exited, new Promise((r) => setTimeout(r, 5000))]);
  if (child.exitCode === null && !child.signalCode) child.kill("SIGKILL");
}

function d1(sql: string): Json[] {
  const r = spawnSync(WRANGLER, ["d1", "execute", "codync-dev", "--local", "--env", "dev", "--persist-to", persist, "--json", "--command", sql], {
    cwd: CLOUD_DIR,
    encoding: "utf8",
  });
  if (r.status !== 0) throw new Error(`d1 failed: ${r.stderr}`);
  return (JSON.parse(r.stdout) as { results: Json[] }[])[0]?.results ?? [];
}

// ---- Clerk test tokens ----

const { publicKey, privateKey } = generateKeyPairSync("rsa", { modulusLength: 2048 });
// @clerk/backend strips PEM armor itself; a single line survives `--var`.
const jwtKey = publicKey
  .export({ type: "spki", format: "pem" })
  .toString()
  .replace(/-----[A-Z ]+-----|\s/g, "");

async function clerkToken(sub: string): Promise<string> {
  const key = await crypto.subtle.importKey("jwk", privateKey.export({ format: "jwk" }), { name: "RSASSA-PKCS1-v1_5", hash: "SHA-256" }, false, [
    "sign",
  ]);
  const now = Math.floor(Date.now() / 1000);
  const part = (v: unknown) => ref.b64url(ref.enc.encode(JSON.stringify(v)));
  const head = `${part({ alg: "RS256", typ: "JWT" })}.${part({ iss: ISSUER, sub, sid: `sess_${sub}`, iat: now - 5, nbf: now - 5, exp: now + 3600, email: `${sub}@example.com` })}`;
  return `${head}.${ref.b64url(new Uint8Array(await crypto.subtle.sign("RSASSA-PKCS1-v1_5", key, ref.enc.encode(head))))}`;
}

async function cloud(method: string, path: string, o: { token?: string; key?: ref.SignKey; body?: unknown } = {}): Promise<{ status: number; body: Json }> {
  const bytes = o.body === undefined ? new Uint8Array() : ref.enc.encode(JSON.stringify(o.body));
  const headers: Record<string, string> = {};
  if (o.token) headers.Authorization = `Bearer ${o.token}`;
  if (o.body !== undefined) headers["Content-Type"] = "application/json";
  if (o.key) headers["Codync-Sig"] = ref.signRequest(o.key, { method, authority: `127.0.0.1:${CLOUD_PORT}`, pathAndQuery: path, body: bytes });
  const res = await fetch(CLOUD + path, { method, headers, body: bytes.length ? bytes : undefined });
  return { status: res.status, body: await res.json() };
}

// ---- the host ----

let hostPort = 0;
let host: ChildProcess | undefined;

async function startHost() {
  hostPort ||= await freePort();
  // 0.0.0.0 so step 7 can prove a non-loopback bearer token is refused.
  host = run(HOST_BIN, ["serve", "--bind", "0.0.0.0", "--port", String(hostPort)], { CODYNC_HOME: home, CODYNC_CLOUD_URL: CLOUD });
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

/** The relay refused the upgrade (the socket never opened). */
const refused = (wire: Wire) => wire.opened.then(
  () => false,
  () => true,
);

const computerOnline = (id: string) => d1(`SELECT online FROM computers WHERE id = '${id}'`)[0]?.online === 1 || undefined;

// ---- scenario ----

async function main() {
  step("1. local D1 + wrangler dev");
  const migrate = spawnSync(WRANGLER, ["d1", "migrations", "apply", "codync-dev", "--local", "--env", "dev", "--persist-to", persist], {
    cwd: CLOUD_DIR,
    encoding: "utf8",
    env: { ...process.env, CI: "1" },
  });
  if (migrate.status !== 0) throw new Error(`migrations failed: ${migrate.stderr}`);
  run(WRANGLER, ["dev", "--env", "dev", "--ip", "127.0.0.1", "--port", String(CLOUD_PORT), "--persist-to", persist, "--var", `CLERK_JWT_KEY:${jwtKey}`]);
  await waitFor("cloud /v1/health", () => fetch(`${CLOUD}/v1/health`).then((r) => r.ok || undefined).catch(() => undefined), 60_000);

  step("2–3. host registers and comes online");
  await startHost();
  const health = await (await fetch(`http://127.0.0.1:${hostPort}/health`)).json();
  const computerId = health.computerId as string;
  check(/^[A-Za-z0-9_-]{22}$/.test(computerId), "host reports a computerId");
  await waitFor("computer row", () => d1(`SELECT id FROM computers WHERE id = '${computerId}'`).length > 0 || undefined);
  await waitFor("DO presence online", () => computerOnline(computerId), 20_000);
  check(true, "D1 row exists and the DO marked it online");

  step("4. QR pairing over the relay, then chat");
  const pairing: PairingInfo = parsePairing((await loopback("pairing")).pairingUrl);
  check(pairing.computerId === computerId && pairing.cloud === CLOUD, "pairing link names this computer and cloud");
  const phone = new FakePhone();
  const paired = await phone.pair(pairing);
  check(paired.computerId === computerId, "pair answered, link closed with 4100");
  // 6c: the normal reconnect is not a pairing socket, so it isn't capped at 16 messages.
  let ch = await phone.relay(CLOUD, computerId, pairing.signKey);
  const hello = await ch.call("hello", {});
  check(hello.computerId === computerId && hello.boxKey === ref.b64url(pairing.boxKey), "inner hello returns computerId and boxKey");
  for (let i = 0; i < 20; i++) await ch.call("sync", { since: 0 });
  check(true, "more than 16 messages on the regular link");

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

  step("6. account: claim, access request with SAS, revocation");
  const userId = "user_e2e";
  const token = await clerkToken(userId);
  const claim = await cloud("POST", "/v1/claims", { token });
  const signed = await loopback("claimSign", { claimId: claim.body.claimId, nonce: claim.body.nonce, userId });
  const done = await cloud("POST", `/v1/claims/${claim.body.claimId}/complete`, { token, body: signed });
  check(done.status === 200 && done.body.computer.computerId === computerId, "computer claimed");
  await waitFor("host sees its owner", async () => (await loopback("cloudStatus")).owner?.userId === userId || undefined);

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

  const started = Date.now();
  await cloud("DELETE", `/v1/computers/${computerId}/grants/${approved.grantId}`, { token });
  const code = await ch2.wire.closed(5000);
  check(code === 4003 && Date.now() - started < 1000, `revoked socket closed with 4003 within 1 s (${Date.now() - started} ms)`);
  check(await refused(phone2.relayWire(CLOUD, computerId)), "phone 2 can't reconnect");
  await stop(host!);
  await startHost();
  await waitFor("host back online", () => computerOnline(computerId), 20_000);
  check(await refused(phone2.relayWire(CLOUD, computerId)), "still refused after the host reconnects");

  step("6b. a grant injected into D1 is ignored by the host");
  const phone3 = new FakePhone();
  await cloud("POST", "/v1/devices", { token, key: phone3.key, body: { name: "Intruder", platform: "ios" } });
  const r3 = await cloud("POST", `/v1/computers/${computerId}/access-requests`, { token, key: phone3.key, body: { commit: ref.b64url(ref.random(32)) } });
  const dev3 = d1(`SELECT id FROM devices WHERE sign_pub = '${phone3.dk}'`)[0].id;
  d1(`INSERT INTO grants(id, computer_id, device_id, scopes, status, created_at) VALUES ('grt_injected', '${computerId}', '${dev3}', '["control","screen"]', 'active', ${Date.now()})`);
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

async function cleanup() {
  await Promise.all(children.map(stop));
  rmSync(tmp, { recursive: true, force: true });
}

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
