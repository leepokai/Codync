// Codync Cloud on Boat, end to end, isolated (docs/guides/cloud-computer-testing.md): a local cloud creates a
// real Boat sandbox, the host on it claims itself, the owner's new phone gets access without a code, the idle
// computer goes to sleep and a mailbox message wakes it. With CODYNC_E2E_UPDATE_TO (a published release above the
// host's own version, which must then be a production release build), an app that needs that host updates it; with
// CODYNC_E2E_UPDATE_NOW=1 (same build), the owner updates it to the latest release from a phone. With
// CODYNC_E2E_COLD=1 (a long run, about an hour): backups, a cold wake after CODYNC_E2E_COLD_MINUTES (31) asleep, a
// restore from the backup once its own snapshot expired, and deleting the snapshot history.
// Run with `npm run e2e:cloud`. Needs BOAT_API_KEY or BOAT_API_KEY_FILE (a key made as users make theirs: limited to
// the `codync` environment, with Codync's actions) and a logged-in `boat` CLI. The test user gives the cloud that key
// as the apps do.
// CODYNC_HOST_LINUX_BIN: the Linux x86_64 host to test, served to the machine at its first boot (without it, the
// released host, which can't run every step until a release has them). Costs a few minutes of Boat compute; the
// sandbox is deleted at the end, also when a step fails, and test machines older than two hours are swept.

import { randomBytes } from "node:crypto";
import { join } from "node:path";
import * as ref from "../ref.ts";
import { boatKey, boatProxy, deleteSandbox, keepForwarded, savedSnapshot, sh, sweep } from "./boat.ts";
import { check, freePort, LocalCloud, lock, ROOT, serveFiles, step, TEST_PREFIX } from "./harness.ts";
import { Channel, FakePhone, waitFor, type Json } from "./phone.ts";

const HOST_LINUX_BIN = process.env.CODYNC_HOST_LINUX_BIN;
const USER_ID = "user_cloud_e2e";
const IDLE_SECS = 20;
const UPDATE_TO = process.env.CODYNC_E2E_UPDATE_TO;
const UPDATE_NOW = process.env.CODYNC_E2E_UPDATE_NOW === "1";
const COLD = process.env.CODYNC_E2E_COLD === "1";
const COLD_MINUTES = Number(process.env.CODYNC_E2E_COLD_MINUTES ?? 31);
const SWEEP_CRON = "*/15 * * * *";

const key = boatKey();
const release = lock("boat");
const cloud = new LocalCloud(await freePort());
const filesPort = await freePort();
const files = serveFiles({ ...(HOST_LINUX_BIN ? { "codync-host": HOST_LINUX_BIN } : {}), "fake_agent.py": join(ROOT, "host/tests/fake_agent.py") }, filesPort);
const proxyPort = await freePort();
const proxy = COLD ? boatProxy(proxyPort) : undefined;
/** The cloud computer's id once created, to delete it at the end. */
let created: string | undefined;
let sandboxId: string | undefined;
/** Sandboxes the cloud stopped tracking (the one a restore replaced), deleted at the end. */
const replaced: string[] = [];

async function main() {
  step("1. local cloud; the user gives it their own Boat key");
  const swept = await sweep(key, 2 * 3600_000);
  if (swept.length) console.log(`  swept test machines left behind: ${swept.join(", ")}`);
  await cloud.start({
    CLOUD_SECRET_KEYS: `k1:${randomBytes(32).toString("base64")}`,
    // A machine a crashed run leaves behind stops by itself; two hours cover the long run.
    CLOUD_AUTO_STOP_SECONDS: "7200",
    CLOUD_MACHINE_PREFIX: TEST_PREFIX,
    // The machine reaches this cloud at its own 127.0.0.1 (a reverse forward), so the host signs the authority the
    // Worker sees. Idle after 20 s instead of 10 minutes; its events marked `env: dev`.
    CLOUD_HOST_ENV: `CODYNC_CLOUD_URL=${cloud.url} CODYNC_DEVICE_CERT_KEY=${cloud.certPub} CODYNC_SLEEP_IDLE_SECS=${IDLE_SECS} CODYNC_ANALYTICS_ENV=dev`,
    ...(HOST_LINUX_BIN ? { CLOUD_HOST_BINARY_URL: `http://127.0.0.1:${filesPort}/codync-host` } : {}),
    ...(proxy ? { BOAT_API_URL: `http://127.0.0.1:${proxyPort}` } : {}),
  });
  const token = await cloud.token(USER_ID);
  check((await cloud.call("POST", "/v1/cloud-computers", { token })).status === 409, "no cloud computer before the user's own key");
  const saved = await cloud.call("PUT", "/v1/cloud-credentials/boat", { token, body: { key } });
  check(saved.body.credential?.hint === `${key.slice(0, key.indexOf("_") + 1)}…${key.slice(-4)}`, `Boat took the key; only its hint comes back (${saved.status})`);
  const stored = (await cloud.d1(`SELECT sealed FROM cloud_credentials WHERE user_id = '${USER_ID}'`))[0]?.sealed as string;
  check(!!stored && !stored.includes(key.slice(5)), "the cloud keeps it sealed");

  step("2. create the cloud computer");
  const started = Date.now();
  const res = await cloud.call("POST", "/v1/cloud-computers", { token, body: { name: "E2E" } });
  check(res.status === 200, `created (${res.status})`);
  const cc = res.body.cloudComputer;
  created = cc.cloudComputerId as string;
  check(cc.providerName === "Boat" && cc.region === "EU" && cc.name === "E2E", "the user sees its name, which provider runs it, and where");
  const id = (await cloud.d1(`SELECT provider_id FROM cloud_machines WHERE id = '${cc.cloudComputerId}'`))[0].provider_id as string;
  sandboxId = id;
  console.log(`  sandbox ${id}`);
  await waitFor("sandbox running", async () => (await cloud.call("GET", "/v1/cloud-computers", { token })).body.cloudComputers[0].state === "running" || undefined, 120_000);
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
  const computerId = await waitFor("claimed", async () => (await cloud.call("GET", "/v1/cloud-computers", { token })).body.cloudComputers[0].computerId ?? undefined, 180_000);
  check(true, `claimed ${computerId} ${Date.now() - hostUp} ms after the host started`);
  check((await sh(id, "test -f ~/.codync/managed.json")).exitCode !== 0, "the ticket is gone after the claim");

  step("5. the owner's new phone gets access without a code");
  const phone = new FakePhone();
  check((await cloud.call("POST", "/v1/devices", { token, key: phone.key, body: { name: "Cloud phone", platform: "ios" } })).status === 200, "phone registered");
  const listed = ((await cloud.call("GET", "/v1/computers", { token, key: phone.key })).body.computers as Json[]).find((c) => c.computerId === computerId);
  check(listed?.access === "none", "the cloud computer is in the account's list");
  if (HOST_LINUX_BIN) {
    // Not the machine's hostname, which changes on every wake (the released host before this build still sends it).
    const named = async () => ((await cloud.call("GET", "/v1/computers", { token })).body.computers as Json[]).find((c) => c.computerId === computerId);
    await waitFor("named", async () => ((c) => (c?.name === "Codync Cloud" && c.device === "cloud") || undefined)(await named()), 30_000);
    check(true, "every app shows it as Codync Cloud with a cloud icon");
  }
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
  // Approvals on `auto`, as new bots start: a bot waiting for the owner's answer counts as working and keeps the
  // computer awake.
  const bot = (await ch.call("createBot", { name: "Sleeper", backend: "custom", command: "python3 /home/user/fake_agent.py", cwd: "/home/user", permission: "auto" })).bot.id as string;
  ch.wire.ws.close();

  const state = async () => (await cloud.call("GET", "/v1/cloud-computers", { token })).body.cloudComputers[0].state as string;
  const machine = async () => (await cloud.d1(`SELECT provider_id, backup FROM cloud_machines WHERE id = '${cc.cloudComputerId}'`))[0]!;
  const asleep = () => waitFor("stopped", async () => ((await state()) === "stopped" ? true : undefined), 300_000);
  /** Puts `text` in the sleeping computer's mailbox and waits until the host the message woke delivers it to the
   * bot. Returns how long that took and the phone's connection, still open (a connected device keeps it awake). */
  async function wakeWith(nonce: string, text: string, ms: number) {
    const wire = phone.relayWire(cloud.url, computerId);
    await wire.opened;
    check((await wire.next((m) => m.t === "presence", "presence")).online === false, "the phone sees it offline");
    // The relay drops a device socket that stays silent for 90 s; a real phone pings every 30 s.
    const keepalive = setInterval(() => wire.send({ t: "ping" }), 30_000);
    const sent = Date.now();
    phone.mailboxPut(wire, { signKey, boxKey: ref.unb64url(hello.boxKey) }, bot, text, nonce);
    check((await wire.next((m) => m.t === "mbox.ok", "mbox.ok")).state === "queued", "queued in the mailbox");
    await wire.next((m) => m.t === "mbox.delivered" && m.nonce === nonce, "mbox.delivered", ms);
    const took = Date.now() - sent;
    const woken = await Channel.open(wire, phone.key, signKey, { relay: true });
    const history = await waitFor("message in history", async () => {
      const h = await woken.call("history", { botId: bot });
      return h.entries.some((e: Json) => e.data?.clientNonce === nonce) ? h : undefined;
    });
    check(!!history, "the bot got the message");
    return { took, history, close: () => (clearInterval(keepalive), wire.ws.close()) };
  }

  step("6. idle, it goes to sleep");
  const closed = Date.now();
  await waitFor("stopping", async () => (["stopping", "stopped"].includes(await state()) ? true : undefined), 180_000);
  check(true, `asked to sleep ${Date.now() - closed} ms after the phone left (idle ${IDLE_SECS} s)`);
  await waitFor("stopped", async () => ((await state()) === "stopped" ? true : undefined), 300_000);
  check(true, `stopped ${Date.now() - closed} ms after the phone left`);

  step("7. a message wakes it");
  const woke = await wakeWith("e2e-wake-1", "hello after sleep", 300_000);
  check(true, `woke and delivered ${woke.took} ms after the message`);
  check((await state()) === "running", "running again");
  woke.close();

  if (COLD) {
    step("7a. asleep, its history stays put, and after 10 days it gets a backup");
    const bootId = async () => (await sh((await machine()).provider_id as string, "cat /proc/sys/kernel/random/boot_id").catch(() => ({ stdout: "?" }))).stdout.trim();
    const bootBefore = await bootId();
    await asleep();
    const stoppedAt = Date.now();
    check((await cloud.call("DELETE", `/v1/cloud-computers/${created}/snapshots`, { token })).status === 409, "asleep, its snapshot history can't be deleted");
    await cloud.d1(`UPDATE cloud_machines SET updated_at = updated_at - ${11 * 86_400_000} WHERE id = '${cc.cloudComputerId}'`);
    await cloud.cron(SWEEP_CRON);
    const backup = await waitFor("backup saved", async () => ((await machine()).backup as string | null) ?? undefined, 120_000);
    check(await savedSnapshot(key, backup), `backup ${backup} saved on Boat from the stopped machine`);

    step(`7b. a cold wake: asleep over ${COLD_MINUTES} minutes, a message brings it back`);
    const left = stoppedAt + COLD_MINUTES * 60_000 - Date.now();
    console.log(`  waiting ${Math.ceil(left / 60_000)} min`);
    await new Promise((r) => setTimeout(r, Math.max(0, left)));
    check((await state()) === "stopped", `still asleep after ${Math.round((Date.now() - stoppedAt) / 60_000)} min`);
    const cold = await wakeWith("e2e-cold-1", "hello after a long sleep", 600_000);
    check(true, `woke and delivered ${cold.took} ms after the message`);
    const bootAfter = await bootId();
    console.log(`  boot id ${bootBefore} → ${bootAfter} (${bootBefore === bootAfter ? "same machine" : "a fresh machine"})`);
    cold.close();

    step("7c. its own snapshot expired (Boat keeps it 14 days): the cloud brings it back from the backup");
    await asleep();
    const before = (await machine()).provider_id as string;
    // The restored machine reaches this computer through its own forward, opened once the cloud names it.
    const watch = setInterval(async () => {
      const now = (await machine().catch(() => undefined))?.provider_id as string | undefined;
      if (!now || now === before || replaced.includes(before)) return;
      replaced.push(before);
      sandboxId = now;
      keepForwarded(cloud, key, now, [cloud.port, filesPort]);
    }, 3000);
    proxy!.expired = true;
    const restored = await wakeWith("e2e-restore-1", "hello from the backup", 600_000);
    proxy!.expired = false;
    clearInterval(watch);
    check(sandboxId !== before, `restored on a new machine (${before} → ${sandboxId}) ${restored.took} ms after the message`);
    check(restored.history.entries.some((e: Json) => e.data?.clientNonce === "e2e-wake-1"), "with the bot's history from the backup");

    step("7d. awake, its snapshot history and backup can be deleted");
    check((await cloud.call("DELETE", `/v1/cloud-computers/${created}/snapshots`, { token })).status === 200, "deleted while awake");
    check((await machine()).backup === null && !(await savedSnapshot(key, backup)), "the backup is gone from Boat");
    check((await state()) === "running", "the computer keeps running");
    restored.close();
  }

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

  if (UPDATE_NOW) {
    step("9. the owner updates it to the latest release from a phone");
    const wire = phone.relayWire(cloud.url, computerId);
    await wire.opened;
    const ping = setInterval(() => wire.send({ t: "ping" }), 30_000);
    // Asleep again after the last step: the phone's connection wakes it.
    const ch = await waitFor("handshake", () => Channel.open(wire, phone.key, signKey, { relay: true }).catch(() => undefined), 300_000);
    const offer = await waitFor("an update offered", async () => {
      const h = await ch.call("hello", {});
      return h.canUpdate === true && typeof h.latestVersion === "string" && h.latestVersion !== h.version ? h : undefined;
    }, 60_000);
    check(true, `${offer.version} offers ${offer.latestVersion}`);
    const before = Date.now();
    // Like on any computer, a working bot holds the update back until the owner says to go ahead (`force`, the
    // bot's session resumes).
    const asked = await ch.call("installHostUpdate", {}).then(() => "scheduled", (e: Error) => e.message);
    if (asked !== "scheduled") {
      check(asked.includes("A bot is working"), `held back while a bot works: ${asked}`);
      check((await ch.call("installHostUpdate", { force: true })).scheduled === true, "the update is scheduled anyway");
    }
    await waitFor("host on the latest release", async () => {
      const c = ((await cloud.call("GET", "/v1/computers", { token, key: phone.key })).body.computers as Json[]).find((c) => c.computerId === computerId);
      return c?.version === offer.latestVersion && c.online ? c : undefined;
    }, 300_000);
    check(true, `updated to ${offer.latestVersion} and back online ${Date.now() - before} ms after asking`);
    clearInterval(ping);
    wire.ws.close();
  }

  step("10. delete");
  check((await cloud.call("DELETE", `/v1/cloud-computers/${created}`, { token })).status === 200, "deleted");
  created = undefined;
  check((await cloud.call("GET", "/v1/cloud-computers", { token })).body.cloudComputers.length === 0, "no cloud computer left");
  const after = (await cloud.call("GET", "/v1/computers", { token, key: phone.key })).body.computers as Json[];
  check(!after.some((c) => c.computerId === computerId), "the computer left the account");

  console.log("\nE2E passed");
}

async function cleanup(failed = false) {
  files.close();
  for (const id of replaced) await deleteSandbox(key, id).catch(() => undefined);
  if (failed) console.error(`\nworker logs:\n${await cloud.logs()}`);
  if (failed && sandboxId) {
    const log = await sh(sandboxId, "journalctl --user -u codync-host -n 60 --no-pager").catch(() => undefined);
    if (log) console.error(`\nhost log:\n${log.stdout}`);
  }
  if (created) {
    const res = await cloud.call("DELETE", `/v1/cloud-computers/${created}`, { token: await cloud.token(USER_ID) }).catch(() => undefined);
    if (res?.status !== 200) console.error("  couldn't delete the sandbox; run `boat list` and delete it by hand");
  }
  // The cloud reaches Boat through the proxy, also for that last delete.
  proxy?.server.close();
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
