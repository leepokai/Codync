// InstaBox for cloud computer end-to-end runs (docs/guides/cloud-computer-testing.md): the key the local cloud uses,
// a sweep of test machines, and a public tunnel to the local cloud. InstaBox machines can't be reached by a reverse port
// forward (its SSH forwards one way only), so they reach the local cloud through a Cloudflare quick tunnel; the Worker
// sees the tunnel's host, which is what the host signs.

import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { type LocalCloud, TEST_PREFIX } from "./harness.ts";

const API = "https://api.instabox.sh";

/** The key from INSTABOX_API_KEY or INSTABOX_API_KEY_FILE, or null when neither is set. */
export function instaboxKey(): string | null {
  const file = process.env.INSTABOX_API_KEY_FILE;
  const key = process.env.INSTABOX_API_KEY ?? (file ? readFileSync(file, "utf8").trim() : "");
  delete process.env.INSTABOX_API_KEY;
  return key || null;
}

/** Deletes test machines (named `codync-e2e-…`) older than `olderThanMs`, whatever their state. */
export async function sweepInstabox(key: string, olderThanMs: number): Promise<string[]> {
  const headers = { authorization: `Bearer ${key}` };
  const res = await fetch(`${API}/v1/machines?limit=200`, { headers });
  if (!res.ok) throw new Error(`InstaBox list: ${res.status}`);
  const { machines } = (await res.json()) as { machines: { id: string; name?: string; createdAt: string }[] };
  const stale = machines.filter((m) => m.name?.startsWith(`${TEST_PREFIX}-`) && Date.now() - Date.parse(m.createdAt) > olderThanMs);
  for (const m of stale) {
    await fetch(`${API}/v1/machines/${m.id}`, {
      method: "DELETE",
      headers: { ...headers, "Idempotency-Key": crypto.randomUUID(), "X-Confirm-Delete": m.id },
    });
  }
  return stale.map((m) => m.id);
}

/** A Cloudflare quick tunnel to the local cloud (or another local `url`): its `https://….trycloudflare.com` URL. Runs
 * with an empty config, so a `~/.cloudflared/config.yml` of the user's own tunnel can't answer for it. */
export async function tunnel(cloud: LocalCloud, url = cloud.url): Promise<string> {
  const config = join(cloud.tmp, "cloudflared.yml");
  writeFileSync(config, "");
  const child = cloud.run("cloudflared", ["tunnel", "--config", config, "--no-autoupdate", "--url", url]);
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("no tunnel URL from cloudflared in 60 s")), 60_000);
    child.stderr?.on("data", (d: Buffer) => {
      const url = /https:\/\/[a-z0-9-]+\.trycloudflare\.com/.exec(String(d))?.[0];
      if (!url) return;
      clearTimeout(timer);
      resolve(url);
    });
  });
}
