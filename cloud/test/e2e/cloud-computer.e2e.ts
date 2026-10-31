// Codync Cloud on Boat, end to end, isolated (docs/guides/cloud-computer-testing.md): a local cloud creates a
// real Boat sandbox, the host on it claims itself, the owner's new phone gets access without a code, the idle
// computer goes to sleep and a mailbox message wakes it. With CODYNC_E2E_UPDATE_TO (a published release above the
// host's own version, which must then be a production release build), an app that needs that host updates it.
// Run with `npm run e2e:cloud`. Needs BOAT_API_KEY or BOAT_API_KEY_FILE (scoped key) and a logged-in `boat` CLI.
// CODYNC_HOST_LINUX_BIN: the Linux x86_64 host to test, served to the machine at its first boot (without it, the
// released host, which can't run every step until a release has them). Costs a few minutes of Boat compute; the
// sandbox is deleted at the end, also when a step fails, and test machines older than two hours are swept.

import { join } from "node:path";
import * as ref from "../ref.ts";
import { boatKey, keepForwarded, sh, sweep, TEST_PREFIX } from "./boat.ts";
import { check, freePort, LocalCloud, lock, ROOT, serveFiles, step } from "./harness.ts";
import { Channel, FakePhone, waitFor, type Json } from "./phone.ts";

const HOST_LINUX_BIN = process.env.CODYNC_HOST_LINUX_BIN;
const USER_ID = "user_cloud_e2e";
const IDLE_SECS = 20;
const UPDATE_TO = process.env.CODYNC_E2E_UPDATE_TO;

const key = boatKey();
const release = lock("boat");
const cloud = new LocalCloud(await freePort());
const filesPort = await freePort();
const files = serveFiles({ ...(HOST_LINUX_BIN ? { "codync-host": HOST_LINUX_BIN } : {}), "fake_agent.py": join(ROOT, "host/tests/fake_agent.py") }, filesPort);
let created = false;
let sandboxId: string | undefined;

async function main() {
  step("1. local cloud with a Boat key");
  const swept = await sweep(key, 2 * 3600_000);
  if (swept.length) console.log(`  swept test machines left behind: ${swept.join(", ")}`);
  await cloud.start({
    BOAT_API_KEY: key,
    CLOUD_COMPUTER_USERS: USER_ID,
    // Boat's trial refuses sandboxes without an auto-stop; an hour covers the run.
    CLOUD_AUTO_STOP_SECONDS: "3600",
    CLOUD_MACHINE_PREFIX: TEST_PREFIX,
    // The machine reaches this cloud at its own 127.0.0.1 (a reverse forward), so the host signs the authority the
    // Worker sees. Idle after 20 s instead of 10 minutes; its events marked `env: dev`.
    CLOUD_HOST_ENV: `CODYNC_CLOUD_URL=${cloud.url} CODYNC_SLEEP_IDLE_SECS=${IDLE_SECS} CODYNC_ANALYTICS_ENV=dev`,
    ...(HOST_LINUX_BIN ? { CLOUD_HOST_BINARY_URL: `http://127.0.0.1:${filesPort}/codync-host` } : {}),
  });
  const token = await cloud.token(USER_ID);
  check((await cloud.call("POST", "/v1/cloud-computers", { token: await cloud.token("user_not_listed") })).status === 403, "a user off the list is refused");

  step("2. create the cloud computer");
  const started = Date.now();
  const res = await cloud.call("POST", "/v1/cloud-computers", { token });
  check(res.status === 200, `created (${res.status})`);
  created = true;
  const cc = res.body.cloudComputer;
  check(cc.providerName === "Boat" && cc.region === "EU", "the user sees which provider runs it, and where");
  check((await cloud.call("POST", "/v1/cloud-computers", { token })).body.cloudComputer.cloudComputerId === cc.cloudComputerId, "asking again returns the same one");
  const id = (await cloud.d1(`SELECT provider_id FROM cloud_machines WHERE id = '${cc.cloudComputerId}'`))[0].provider_id as string;
  sandboxId = id;
  console.log(`  sandbox ${id}`);
  await waitFor("sandbox running", async () => (await cloud.call("GET", "/v1/cloud-computers", { token })).body.cloudComputer.state === "running" || undefined, 120_000);
  check(true, `running after ${Date.now() - started} ms`);

  step("3. the machine reaches this computer, its first boot installs the host under test");
  keepForwarded(cloud, key, id, [cloud.port, filesPort]);
  const installed = await waitFor("host service active", async () => ((await sh(id, "systemctl --user is-active codync-host")).stdout.trim() === "active" ? true : undefined), 300_000);
  check(installed, `setup finished after ${Date.now() - started} ms`);
  const hostUp = Date.now();
  if (HOST_LINUX_BIN) check((await sh(id, "codync-host --version")).stdout.trim() !== "", "the host under test runs from the first boot");
  // A fake agent for the bot of step 5.
  check((await sh(id, `curl -fsS http://127.0.0.1:${filesPort}/fake_agent.py -o ~/fake_agent.py`)).exitCode === 0, "fake agent on the machine");

  step("4. the host claims itself");
  const computerId = await waitFor("claimed", async () => (await cloud.call("GET", "/v1/cloud-computers", { token })).body.cloudComputer.computerId ?? undefined, 180_000);
  check(true, `claimed ${computerId} ${Date.now() - hostUp} ms after the host started`);
  check((await sh(id, "test -f ~/.codync/managed.json")).exitCode !== 0, "the ticket is gone after the claim");

  step("5. the owner's new phone gets access without a code");
  const phone = new FakePhone();
  check((await cloud.call("POST", "/v1/devices", { token, key: phone.key, body: { name: "Cloud phone", platform: "ios" } })).status === 200, "phone registered");
  const listed = ((await cloud.call("GET", "/v1/computers", { token, key: phone.key })).body.computers as Json[]).find((c) => c.computerId === computerId);
  check(listed?.access === "none", "the cloud computer is in the account's list");
  const nD = ref.random(32);
  const req = await cloud.call("POST", `/v1/computers/${computerId}/access-requests`, {
    token,
    key: phone.key,
    body: { commit: ref.b64url(ref.sasCommit(phone.key.pub, nD)) },
  });
  const requestId = req.body.requestId as string;
  await waitFor("host nonce", async () => (await cloud.call("GET", `/v1/access-requests/${requestId}`, { token })).body.hostNonce, 30_000);
  await cloud.call("POST", `/v1/access-requests/${requestId}/reveal`, { token, key: phone.key, body: { nonce: ref.b64url(nD) } });
  await waitFor("auto-approved", async () => (await cloud.call("GET", `/v1/access-requests/${requestId}`, { token })).body.status === "approved" || undefined, 30_000);
  // The host lets the device in on the relay right after it approves; until then the socket is refused.
  const signKey = ref.unb64url(listed.signKey);
  const ch = await waitFor("relay socket", () => phone.relay(cloud.url, computerId, signKey).catch(() => undefined), 30_000);
  const hello = await ch.call("hello", {});
  check(hello.computerId === computerId, `connected through the relay ${Date.now() - started} ms after create`);
  const bot = (await ch.call("createBot", { name: "Sleeper", backend: "custom", command: "python3 /home/user/fake_agent.py", cwd: "/home/user", permission: "ask" })).bot.id as string;
  ch.wire.ws.close();

  step("6. idle, it goes to sleep");
  const closed = Date.now();
  const state = async () => (await cloud.call("GET", "/v1/cloud-computers", { token })).body.cloudComputer.state as string;
  await waitFor("stopping", async () => (["stopping", "stopped"].includes(await state()) ? true : undefined), 180_000);
  check(true, `asked to sleep ${Date.now() - closed} ms after the phone left (idle ${IDLE_SECS} s)`);
  await waitFor("stopped", async () => ((await state()) === "stopped" ? true : undefined), 300_000);
  check(true, `stopped ${Date.now() - closed} ms after the phone left`);

  step("7. a message wakes it");
  const wire = phone.relayWire(cloud.url, computerId);
  await wire.opened;
  check((await wire.next((m) => m.t === "presence", "presence")).online === false, "the phone sees it offline");
  // The relay drops a device socket that stays silent for 90 s; a real phone pings every 30 s.
  const keepalive = setInterval(() => wire.send({ t: "ping" }), 30_000);
  const sent = Date.now();
  phone.mailboxPut(wire, { signKey, boxKey: ref.unb64url(hello.boxKey) }, bot, "hello after sleep", "e2e-wake-1");
  check((await wire.next((m) => m.t === "mbox.ok", "mbox.ok")).state === "queued", "queued in the mailbox");
  await wire.next((m) => m.t === "mbox.delivered" && m.nonce === "e2e-wake-1", "mbox.delivered", 300_000);
  clearInterval(keepalive);
  check(true, `woke and delivered ${Date.now() - sent} ms after the message`);
  check((await state()) === "running", "running again");
  const woken = await Channel.open(wire, phone.key, signKey, { relay: true });
  const history = await waitFor("message in history", async () => {
    const h = await woken.call("history", { botId: bot });
    return h.entries.some((e: Json) => e.data?.clientNonce === "e2e-wake-1") ? h : undefined;
  });
  check(!!history, "the bot got the message");
  wire.ws.close();

  if (UPDATE_TO) {
    step("8. an app that needs a newer host updates the cloud computer first");
    const before = Date.now();
    const needy = phone.relayWire(cloud.url, computerId, undefined, { "Codync-App": `ios/9.0.0; minHost=${UPDATE_TO}` });
    await needy.opened;
    const ping = setInterval(() => needy.send({ t: "ping" }), 30_000);
    const computer = async () =>
      ((await cloud.call("GET", "/v1/computers", { token, key: phone.key })).body.computers as Json[]).find((c) => c.computerId === computerId);
    await waitFor("host on the new version", async () => {
      const c = await computer();
      return c?.version === UPDATE_TO && c.online ? c : undefined;
    }, 300_000);
    check(true, `updated to ${UPDATE_TO} and back online ${Date.now() - before} ms after the app connected`);
    const updated = await waitFor("handshake with the new host", () => Channel.open(needy, phone.key, signKey, { relay: true }).catch(() => undefined), 60_000);
    check((await updated.call("hello", {})).computerId === computerId, "the app talks to the updated host");
    clearInterval(ping);
    needy.ws.close();
  }

  step("9. delete");
  check((await cloud.call("DELETE", "/v1/cloud-computers", { token })).status === 200, "deleted");
  created = false;
  check((await cloud.call("GET", "/v1/cloud-computers", { token })).body.cloudComputer === null, "no cloud computer left");
  const after = (await cloud.call("GET", "/v1/computers", { token, key: phone.key })).body.computers as Json[];
  check(!after.some((c) => c.computerId === computerId), "the computer left the account");

  console.log("\nE2E passed");
}

async function cleanup(failed = false) {
  files.close();
  if (failed && sandboxId) {
    const log = await sh(sandboxId, "journalctl --user -u codync-host -n 60 --no-pager").catch(() => undefined);
    if (log) console.error(`\nhost log:\n${log.stdout}`);
  }
  if (created) {
    const res = await cloud.call("DELETE", "/v1/cloud-computers", { token: await cloud.token(USER_ID) }).catch(() => undefined);
    if (res?.status !== 200) console.error("  couldn't delete the sandbox; run `boat list` and delete it by hand");
  }
  await cloud.cleanup();
  release();
}

try {
  await main();
  await cleanup();
} catch (e) {
  console.error(`\nE2E failed: ${e instanceof Error ? e.stack : e}`);
  await cleanup(true);
  process.exit(1);
}
