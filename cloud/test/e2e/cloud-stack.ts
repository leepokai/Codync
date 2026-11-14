// A local cloud for the real Codync Dev apps (iOS simulator, Android emulator, desktop sandbox), isolated from the
// shared dev cloud (docs/guides/cloud-computer-testing.md): it accepts dev Clerk sign-ins and creates cloud computers
// on the provider each tester gives a key in the app. Boat machines (BOAT_API_KEY or BOAT_API_KEY_FILE, as for
// `npm run e2e:cloud`) boot the host under test and are forwarded to this computer; InstaBox machines
// (INSTABOX_API_KEY or INSTABOX_API_KEY_FILE) reach this cloud through a Cloudflare quick tunnel, which then serves
// every machine, and fetch CODYNC_HOST_LINUX_BIN (else the released host) through a second one. Ctrl-C deletes the
// test machines and stops everything. Run with `npm run cloud:stack`; also: CODYNC_STACK_PORT (a fixed port, so a
// simulator launched once keeps working), CODYNC_SLEEP_IDLE_SECS (a short idle time to watch sleep happen). Without a
// provider key it is a plain local cloud with cloud computers off, as a worktree sandbox runs it (`npm run dev` in
// apps/desktop, which also sets CODYNC_STACK_DIR, a folder that keeps its D1 and device certificate key across runs, and
// CODYNC_STACK_INFO, a file this writes `{url, certPub}` to once the cloud answers).

import { randomBytes } from "node:crypto";
import { writeFileSync } from "node:fs";
import { join } from "node:path";
import { boatKey, keepForwarded, sweep } from "./boat.ts";
import { freePort, LocalCloud, lock, ROOT, serveFiles, TEST_PREFIX } from "./harness.ts";
import { instaboxKey, sweepInstabox, tunnel } from "./instabox.ts";

const HOST_LINUX_BIN = process.env.CODYNC_HOST_LINUX_BIN;
const idle = process.env.CODYNC_SLEEP_IDLE_SECS;

const ibx = instaboxKey();
const key = process.env.BOAT_API_KEY || process.env.BOAT_API_KEY_FILE ? boatKey() : null;
const providers = Boolean(key || ibx);
const release = providers ? lock("boat") : () => {};
const cloud = new LocalCloud(Number(process.env.CODYNC_STACK_PORT ?? (await freePort())), { keep: process.env.CODYNC_STACK_DIR });
const filesPort = await freePort();
const files = serveFiles({ ...(HOST_LINUX_BIN ? { "codync-host": HOST_LINUX_BIN } : {}), "fake_agent.py": join(ROOT, "host/tests/fake_agent.py") }, filesPort);

// Set before anything starts, so a signal or a failed start stops all of it.
let watch: ReturnType<typeof setInterval> | undefined;
let stopping = false;
const shutdown = async (code = 0) => {
  if (stopping) return;
  stopping = true;
  clearInterval(watch);
  console.log(providers ? "\nstopping: deleting the test machines" : "\nstopping");
  if (key) await sweep(key, 0).catch((e) => console.error(String(e)));
  if (ibx) await sweepInstabox(ibx, 0).catch((e) => console.error(String(e)));
  files.close();
  await cloud.cleanup();
  release();
  process.exit(code);
};
for (const signal of ["SIGINT", "SIGTERM"] as const) process.on(signal, () => void shutdown());

if (key) await sweep(key, 2 * 3600_000);
if (ibx) await sweepInstabox(ibx, 2 * 3600_000);
// The tunnel starts before the Worker it serves: its URL goes in the hosts' environment.
const hostCloudUrl = ibx ? await tunnel(cloud) : cloud.url;
// Boat's forwards reach the file server on this computer; InstaBox machines fetch it through a tunnel.
const filesUrl = HOST_LINUX_BIN && (ibx ? await tunnel(cloud, `http://127.0.0.1:${filesPort}`) : `http://127.0.0.1:${filesPort}`);
await cloud.start(
  {
    // Without a provider key nobody gets a cloud computer (no sealing keys): one made here could never reach this cloud.
    ...(providers ? { CLOUD_SECRET_KEYS: `k1:${randomBytes(32).toString("base64")}` } : {}),
    CLOUD_AUTO_STOP_SECONDS: "7200",
    CLOUD_MACHINE_PREFIX: TEST_PREFIX,
    CLOUD_HOST_ENV: [`CODYNC_CLOUD_URL=${hostCloudUrl}`, `CODYNC_DEVICE_CERT_KEY=${cloud.certPub}`, "CODYNC_ANALYTICS_ENV=dev", ...(idle ? [`CODYNC_SLEEP_IDLE_SECS=${idle}`] : [])].join(" "),
    ...(filesUrl ? { CLOUD_HOST_BINARY_URL: `${filesUrl}/codync-host` } : {}),
  },
  { clerk: "dev" },
).catch(async (e: unknown) => {
  console.error(String(e));
  await shutdown(1);
});

const port = cloud.port;
if (process.env.CODYNC_STACK_INFO) writeFileSync(process.env.CODYNC_STACK_INFO, JSON.stringify({ url: cloud.url, certPub: cloud.certPub }));
console.log(`
Local cloud for Codync Dev apps: ${cloud.url} (dev Clerk sign-ins; test account: any name+clerk_test@example.com, code 424242)
  iOS simulator:    SIMCTL_CHILD_CODYNC_CLOUD_URL=${cloud.url} xcrun simctl launch <udid> com.pokai.Codync.ios.dev
  Desktop sandbox:  CODYNC_CLOUD_URL=${cloud.url} CODYNC_DEVICE_CERT_KEY=${cloud.certPub} npm run dev -- --cloud   (in apps/desktop)
  Android emulator: adb -s <serial> reverse tcp:${port} tcp:${port}, built with -PcodyncCloudUrl=${cloud.url} plus -PcodyncClerkPublishableKey and -PcodyncPushRelayUrl
${
  providers
    ? `Cloud computers: ${HOST_LINUX_BIN ? `boot ${HOST_LINUX_BIN}` : "boot the released host"}; ${ibx ? `they reach this cloud at ${hostCloudUrl}` : "each new one is forwarded here"}.
Keys to paste in the app: ${[key && "Boat: the one in BOAT_API_KEY_FILE (limited to the codync environment)", ibx && "InstaBox: the one in INSTABOX_API_KEY_FILE"].filter(Boolean).join("; ")}.`
    : "Cloud computers: off (start with BOAT_API_KEY_FILE or INSTABOX_API_KEY_FILE to test them)."
} Ctrl-C to stop.
`);

const forwarded = new Set<string>();
watch = setInterval(async () => {
  if (!key) return;
  const rows = await cloud.d1("SELECT provider_id FROM cloud_machines WHERE provider = 'boat' AND state != 'deleted'").catch(() => []);
  for (const row of rows) {
    const id = row.provider_id as string;
    if (forwarded.has(id)) continue;
    forwarded.add(id);
    keepForwarded(cloud, key, id, [port, filesPort]);
    console.log(`cloud computer ${id}: forwarded`);
  }
}, 5000);
