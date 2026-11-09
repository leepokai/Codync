// Published host releases and the oldest app each works with, for the cloud soft update (`app-versions.ts`).
// The cron reads GitHub's latest mark and tags, and each new release's `MIN_APP` from its own source
// (`host/src/compat.rs` at the tag), so releases need no extra asset. Releases above the latest mark (held by
// the release gate, or moved back) are dropped.

import { compareVersions } from "./app-versions";
import type { Env } from "./index";

const REPO = "leepokai/Codync";
const TAG = /^v(\d{1,4}\.\d{1,5}\.\d{1,6})$/;

export async function syncReleases(env: Env): Promise<void> {
  const headers: Record<string, string> = { Accept: "application/vnd.github+json", "User-Agent": "codync-cloud" };
  if (env.RELEASE_GATE_TOKEN) headers.Authorization = `Bearer ${env.RELEASE_GATE_TOKEN}`;
  const get = async <T>(path: string): Promise<T> => {
    const res = await fetch(`https://api.github.com/repos/${REPO}${path}`, { headers });
    if (!res.ok) throw new Error(`GitHub ${path}: ${res.status}`);
    return (await res.json()) as T;
  };
  const latest = TAG.exec((await get<{ tag_name?: string }>("/releases/latest")).tag_name ?? "")?.[1];
  if (!latest) return;
  const tags = (await get<{ name: string }[]>("/tags?per_page=30"))
    .map((t) => TAG.exec(t.name)?.[1])
    .filter((v): v is string => !!v && compareVersions(v, latest) <= 0);
  const known = new Set(
    (await env.DB.prepare("SELECT version FROM host_releases").all<{ version: string }>()).results.map((r) => r.version),
  );
  const stmts = [...known].filter((v) => compareVersions(v, latest) > 0).map((v) => env.DB.prepare("DELETE FROM host_releases WHERE version = ?").bind(v));
  for (const version of tags.filter((v) => !known.has(v))) {
    const res = await fetch(`https://raw.githubusercontent.com/${REPO}/v${version}/host/src/compat.rs`);
    const minApp = res.ok ? /pub const MIN_APP: &str = "(\d+\.\d+\.\d+)"/.exec(await res.text())?.[1] : undefined;
    if (minApp) stmts.push(env.DB.prepare("INSERT OR IGNORE INTO host_releases(version, min_app) VALUES (?, ?)").bind(version, minApp));
  }
  if (stmts.length) await env.DB.batch(stmts);
}
