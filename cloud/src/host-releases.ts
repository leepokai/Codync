// Where this cloud's computers install and update their host, and the releases the cloud soft update picks from
// (`app-versions.ts`). A channel has GitHub's layout: GitHub's releases on main, the dev channel on dev
// (`HOST_RELEASES_URL`, docs/guides/development.md#dev-release-channel). The cron reads the channel's latest
// `codync-host-compat.json` (the version and its `MIN_APP`) and keeps each version it sees; versions above the
// latest (held back by the release gate, or moved back) are dropped.

import { compareVersions } from "./app-versions";
import type { Env } from "./index";

const GITHUB_RELEASES = "https://github.com/leepokai/Codync/releases";
const CHANNEL = /^https:\/\/[A-Za-z0-9.-]+(?:\/[A-Za-z0-9._-]+)*$/;
const VERSION = /^\d{1,4}\.\d{1,5}\.\d{1,6}$/;

/** The release channel this cloud's computers use. */
export function releasesUrl(env: Env): string {
  const url = env.HOST_RELEASES_URL;
  if (!url) return GITHUB_RELEASES;
  if (!CHANNEL.test(url)) throw new Error("Invalid HOST_RELEASES_URL");
  return url;
}

export async function syncReleases(env: Env): Promise<void> {
  const res = await fetch(`${releasesUrl(env)}/latest/download/codync-host-compat.json`, { headers: { "User-Agent": "codync-cloud" } });
  if (!res.ok) throw new Error(`latest release: ${res.status}`);
  const { version, minApp } = (await res.json()) as { version?: unknown; minApp?: unknown };
  if (typeof version !== "string" || !VERSION.test(version) || typeof minApp !== "string" || !VERSION.test(minApp)) {
    throw new Error("latest release: invalid codync-host-compat.json");
  }
  const known = (await env.DB.prepare("SELECT version FROM host_releases").all<{ version: string }>()).results;
  await env.DB.batch([
    ...known
      .filter((r) => compareVersions(r.version, version) > 0)
      .map((r) => env.DB.prepare("DELETE FROM host_releases WHERE version = ?").bind(r.version)),
    // A dev channel version rebuilt with another MIN_APP replaces its row.
    env.DB.prepare("INSERT INTO host_releases(version, min_app) VALUES (?, ?) ON CONFLICT(version) DO UPDATE SET min_app = excluded.min_app").bind(
      version,
      minApp,
    ),
  ]);
}
