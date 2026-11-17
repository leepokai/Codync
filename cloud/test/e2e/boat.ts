// Boat for cloud computer end-to-end runs (docs/guides/cloud-computer-testing.md): the scoped key the local
// cloud uses, the `boat` CLI (its own login) for commands and reverse port forwards, a sweep of test
// machines a crashed run left behind, and a pass-through to Boat's API that can expire a machine's snapshot.

import { execFile } from "node:child_process";
import { createServer, type Server } from "node:http";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { type LocalCloud, TEST_PREFIX } from "./harness.ts";

const BOAT = process.env.BOAT_BIN ?? join(homedir(), ".ascii/bin/boat");
const API = "https://boat.dev/api/v1";

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

/** Whether Boat has the saved snapshot (a backup) `name`. */
export async function savedSnapshot(key: string, name: string): Promise<boolean> {
  const res = await fetch(`${API}/saved-snapshots/${encodeURIComponent(name)}`, { headers: { authorization: `Bearer ${key}` } });
  if (res.status === 404) return false;
  if (!res.ok) throw new Error(`Boat saved snapshot: ${res.status}`);
  return true;
}

/** Deletes a test machine by id (one the cloud no longer tracks, such as the sandbox a restore replaced). */
export async function deleteSandbox(key: string, id: string): Promise<void> {
  await fetch(`${API}/sandboxes/${encodeURIComponent(id)}`, { method: "DELETE", headers: { authorization: `Bearer ${key}`, "X-Ascii-Confirm-Delete": id } });
}

/**
 * The cloud's way to Boat (`BOAT_API_URL`), passed through unchanged until `expired` is set: then a resume gets
 * Boat's answer for a machine stopped over 14 days (`409 snapshot_data_expired`), so the cloud restores it from
 * its backup on the real Boat.
 */
export function boatProxy(port: number): { expired: boolean; server: Server; seen: Map<string, string> } {
  const proxy = { expired: false, server: undefined as unknown as Server, seen: new Map<string, string>() };
  proxy.server = createServer(async (req, res) => {
    try {
      const chunks: Buffer[] = [];
      for await (const c of req) chunks.push(c as Buffer);
      if (proxy.expired && req.method === "POST" && /^\/sandboxes\/[^/]+\/resume$/.test(req.url ?? "")) {
        console.log(`  [boat api] ${req.method} ${req.url} → 409 snapshot_data_expired (made up)`);
        res.writeHead(409, { "content-type": "application/json" });
        res.end(JSON.stringify({ code: "snapshot_data_expired", message: "The snapshot expired (e2e)." }));
        return;
      }
      const headers: Record<string, string> = {};
      for (const [k, v] of Object.entries(req.headers)) if (typeof v === "string" && !["host", "connection", "content-length"].includes(k)) headers[k] = v;
      const up = await fetch(API + req.url, { method: req.method, headers, body: chunks.length ? Buffer.concat(chunks) : undefined });
      const body = Buffer.from(await up.arrayBuffer());
      // Every call, but a state poll only when the state it reads changes: a failed step shows what the cloud
      // asked of Boat and what it saw.
      const seen = req.method === "GET" && up.ok ? stateOf(body) : undefined;
      if (seen === undefined || proxy.seen.get(req.url ?? "") !== seen) {
        if (seen !== undefined) proxy.seen.set(req.url ?? "", seen);
        console.log(`  [boat api] ${req.method} ${req.url} → ${up.status}${seen ? ` ${seen}` : up.ok ? "" : ` ${body.toString().slice(0, 200)}`}`);
      }
      res.writeHead(up.status, { "content-type": up.headers.get("content-type") ?? "application/json" });
      res.end(body);
    } catch (e) {
      res.writeHead(502, { "content-type": "application/json" });
      res.end(JSON.stringify({ code: "proxy", message: String(e) }));
    }
  }).listen(port, "127.0.0.1");
  return proxy;
}

/** A sandbox answer's state (and the status of its latest snapshot), for the proxy's log. */
function stateOf(body: Buffer): string | undefined {
  try {
    const s = (JSON.parse(body.toString()) as { sandbox?: { state?: string; lastSnapshotStatus?: string } }).sandbox;
    return s?.state ? `${s.state}${s.lastSnapshotStatus ? ` (snapshot ${s.lastSnapshotStatus})` : ""}` : undefined;
  } catch {
    return undefined;
  }
}
