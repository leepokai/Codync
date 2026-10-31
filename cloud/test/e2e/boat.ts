// Boat for cloud computer end-to-end runs (docs/guides/cloud-computer-testing.md): the scoped key the local
// cloud uses, the `boat` CLI (its own login) for commands and reverse port forwards, and a sweep of test
// machines a crashed run left behind.

import { execFile } from "node:child_process";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import type { LocalCloud } from "./harness.ts";

const BOAT = process.env.BOAT_BIN ?? join(homedir(), ".ascii/bin/boat");
const API = "https://boat.dev/api/v1";
/** Test machines are named with this prefix (the cloud's `CLOUD_MACHINE_PREFIX`). */
export const TEST_PREFIX = "codync-e2e";

/** The scoped key from BOAT_API_KEY or BOAT_API_KEY_FILE; removed from the environment, so the CLI uses its login. */
export function boatKey(): string {
  const file = process.env.BOAT_API_KEY_FILE;
  const key = process.env.BOAT_API_KEY ?? (file ? readFileSync(file, "utf8").trim() : "");
  if (!key) throw new Error("set BOAT_API_KEY (or BOAT_API_KEY_FILE) to a scoped Boat key");
  delete process.env.BOAT_API_KEY;
  return key;
}

/** Runs `script` on the sandbox. `boat exec` exits with the script's code and prints one JSON result. */
export function sh(id: string, script: string): Promise<{ exitCode: number; stdout: string }> {
  const args = ["--no-update", "exec", id, "--json", "--timeout", "120", "--", "bash", "-lc", script];
  return new Promise((resolve, reject) =>
    execFile(BOAT, args, { encoding: "utf8", timeout: 150_000 }, (err, stdout, stderr) => {
      try {
        const out = JSON.parse(stdout.trim().split("\n").pop()!);
        resolve({ exitCode: out.exitCode, stdout: out.stdout ?? "" });
      } catch {
        reject(new Error(`boat exec failed: ${stderr || err?.message}`));
      }
    }),
  );
}

/** Makes this computer's `ports` reachable at the same ports inside the sandbox, while `cloud` runs. A long stop
 * drops the tunnel, so it is opened each time the sandbox comes up: its first boot and every wake. */
export function keepForwarded(cloud: LocalCloud, key: string, id: string, ports: number[]): void {
  let forwarded = false;
  const timer = setInterval(async () => {
    const up = await isUp(key, id).catch(() => undefined);
    if (up === false) forwarded = false;
    if (!up || forwarded) return;
    forwarded = true;
    for (const port of ports) cloud.run(BOAT, ["--no-update", "forward", id, "--reverse", "--local", String(port)]);
  }, 5000);
  timer.unref();
}

/** Whether the sandbox is up (Boat's `ready`, `idle` or `running`). */
async function isUp(key: string, id: string): Promise<boolean> {
  const res = await fetch(`${API}/sandboxes/${encodeURIComponent(id)}`, { headers: { authorization: `Bearer ${key}` } });
  if (!res.ok) throw new Error(`Boat get: ${res.status}`);
  const { sandbox } = (await res.json()) as { sandbox?: { state?: string } };
  return ["ready", "idle", "running"].includes(sandbox?.state ?? "");
}

/** Deletes test machines older than `olderThanMs`: what runs that crashed before their cleanup left running. */
export async function sweep(key: string, olderThanMs: number): Promise<string[]> {
  const headers = { authorization: `Bearer ${key}` };
  // Stopped machines too: they keep their disk and still count toward the account's limits.
  const states = "init,provisioning,provisioned,cloning,ready,idle,running,archiving,archived,error";
  const res = await fetch(`${API}/sandboxes?limit=200&state=${states}`, { headers });
  if (!res.ok) throw new Error(`Boat list: ${res.status}`);
  const { sandboxes } = (await res.json()) as { sandboxes: { id: string; name?: string; createdAt: string }[] };
  const stale = sandboxes.filter((s) => s.name?.startsWith(`${TEST_PREFIX}-`) && Date.now() - Date.parse(s.createdAt) > olderThanMs);
  for (const s of stale) {
    await fetch(`${API}/sandboxes/${s.id}`, { method: "DELETE", headers: { ...headers, "X-Ascii-Confirm-Delete": s.id } });
  }
  return stale.map((s) => s.id);
}
