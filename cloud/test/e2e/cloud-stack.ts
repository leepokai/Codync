// A local cloud for the real Codync Dev apps (iOS simulator, Android emulator, desktop sandbox), isolated from the
// shared dev cloud (docs/guides/cloud-computer-testing.md): it accepts dev Clerk sign-ins and creates cloud computers
// on the provider each tester gives a key in the app. Boat machines (BOAT_API_KEY or BOAT_API_KEY_FILE, as for
// `npm run e2e:cloud`) boot the host under test and are forwarded to this computer; InstaBox machines
// (INSTABOX_API_KEY or INSTABOX_API_KEY_FILE) reach this cloud through a Cloudflare quick tunnel, which then serves
// every machine, and fetch CODYNC_HOST_LINUX_BIN (else the released host) through a second one. Ctrl-C deletes the
// test machines and stops everything. Run with `npm run cloud:stack`; also: CLOUD_COMPUTER_USERS (dev Clerk user ids
// allowed a cloud computer, default `*`), CODYNC_STACK_PORT (a fixed port, so a simulator launched once keeps
// working), CODYNC_SLEEP_IDLE_SECS (a short idle time to watch sleep happen).

import { randomBytes } from "node:crypto";
import { join } from "node:path";
import { boatKey, keepForwarded, sweep } from "./boat.ts";
import { freePort, LocalCloud, lock, ROOT, serveFiles, TEST_PREFIX } from "./harness.ts";
import { instaboxKey, sweepInstabox, tunnel } from "./instabox.ts";

const HOST_LINUX_BIN = process.env.CODYNC_HOST_LINUX_BIN;
const idle = process.env.CODYNC_SLEEP_IDLE_SECS;

const ibx = instaboxKey();
// Boat when its key is given, or when nothing else is.
const wantsBoat = Boolean(process.env.BOAT_API_KEY || process.env.BOAT_API_KEY_FILE) || !ibx;
const key = wantsBoat ? boatKey() : null;
const release = lock("boat");
const cloud = new LocalCloud(Number(process.env.CODYNC_STACK_PORT ?? (await freePort())));
const filesPort = await freePort();
const files = serveFiles({ ...(HOST_LINUX_BIN ? { "codync-host": HOST_LINUX_BIN } : {}), "fake_agent.py": join(ROOT, "host/tests/fake_agent.py") }, filesPort);

if (key) await sweep(key, 2 * 3600_000);
if (ibx) await sweepInstabox(ibx, 2 * 3600_000);
// The tunnel starts before the Worker it serves: its URL goes in the hosts' environment.
const hostCloudUrl = ibx ? await tunnel(cloud) : cloud.url;
// Boat's forwards reach the file server on this computer; InstaBox machines fetch it through a tunnel.
const filesUrl = HOST_LINUX_BIN && (ibx ? await tunnel(cloud, `http://127.0.0.1:${filesPort}`) : `http://127.0.0.1:${filesPort}`);
await cloud.start(
  {
    CLOUD_SECRET_KEYS: `k1:${randomBytes(32).toString("base64")}`,
    CLOUD_COMPUTER_USERS: process.env.CLOUD_COMPUTER_USERS ?? "*",
    CLOUD_AUTO_STOP_SECONDS: "7200",
    CLOUD_MACHINE_PREFIX: TEST_PREFIX,
    CLOUD_HOST_ENV: [`CODYNC_CLOUD_URL=${hostCloudUrl}`, `CODYNC_DEVICE_CERT_KEY=${cloud.certPub}`, "CODYNC_ANALYTICS_ENV=dev", ...(idle ? [`CODYNC_SLEEP_IDLE_SECS=${idle}`] : [])].join(" "),
    ...(filesUrl ? { CLOUD_HOST_BINARY_URL: `${filesUrl}/codync-host` } : {}),
  },
  { clerk: "dev" },
);

const port = cloud.port;
console.log(`
Local cloud for Codync Dev apps: ${cloud.url} (dev Clerk sign-ins; test account: any name+clerk_test@example.com, code 424242)
  iOS simulator:    SIMCTL_CHILD_CODYNC_CLOUD_URL=${cloud.url} xcrun simctl launch <udid> com.pokai.Codync.ios.dev
  Desktop sandbox:  CODYNC_CLOUD_URL=${cloud.url} npm run dev   (in apps/desktop)
  Android emulator: adb -s <serial> reverse tcp:${port} tcp:${port}, built with -PcodyncCloudUrl=${cloud.url} plus -PcodyncClerkPublishableKey and -PcodyncPushRelayUrl
Cloud computers: ${HOST_LINUX_BIN ? `boot ${HOST_LINUX_BIN}` : "boot the released host"}; ${ibx ? `they reach this cloud at ${hostCloudUrl}` : "each new one is forwarded here"}. Ctrl-C to stop.
Keys to paste in the app: ${[key && "Boat: the one in BOAT_API_KEY_FILE (limited to the codync environment)", ibx && "InstaBox: the one in INSTABOX_API_KEY_FILE"].filter(Boolean).join("; ")}.
`);

const forwarded = new Set<string>();
const watch = setInterval(async () => {
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

let stopping = false;
process.on("SIGINT", async () => {
  if (stopping) return;
  stopping = true;
  clearInterval(watch);
  console.log("\nstopping: deleting the test machines");
  if (key) await sweep(key, 0).catch((e) => console.error(String(e)));
  if (ibx) await sweepInstabox(ibx, 0).catch((e) => console.error(String(e)));
  files.close();
  await cloud.cleanup();
  release();
  process.exit(0);
});
