// Runs the release gate (.github/workflows/release-gate.yml) every 5 minutes while a published release
// waits behind GitHub's latest mark, for App Review (its MIN_APP is ahead of the App Store) or for its
// last files, and while the Homebrew tap trails the latest release (a tap update that failed after the
// mark moved). The App Store's iPhone app asks for its own host the moment it's out
// (docs/reference/compatibility.md, "Holding releases for the App Store"), and GitHub ran the
// workflow's hourly schedule late or not at all, so this replaced it.
// The gate reads the App Store itself, on GitHub's runners: Apple refuses the lookup from Workers (403).
// Without a token (`RELEASE_GATE_TOKEN`, Actions: write on the repository) it does nothing.

import type { Env } from "./index";

const DISPATCH = "https://api.github.com/repos/leepokai/Codync/actions/workflows/release-gate.yml/dispatches";
// One small request a tick: the REST release list carries every asset (megabytes).
const STATE = `{
  codync: repository(owner: "leepokai", name: "Codync") {
    latestRelease { tagName }
    releases(first: 10, orderBy: { field: CREATED_AT, direction: DESC }) { nodes { tagName isDraft isPrerelease } }
  }
  tap: repository(owner: "leepokai", name: "homebrew-codync") {
    cask: object(expression: "HEAD:Casks/codync.rb") { ... on Blob { text } }
  }
}`;

/** `a` is a later `major.minor.patch` than `b` (a leading `v` is ignored); false for anything unreadable. */
export function isNewer(a: string, b: string): boolean {
  const [x, y] = [a, b].map((v) => /^v?(\d+)\.(\d+)\.(\d+)$/.exec(v)?.slice(1).map(Number));
  if (!x || !y) return false;
  for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] > y[i];
  return false;
}

export type Release = { tagName: string; isDraft?: boolean; isPrerelease?: boolean };
export type State = {
  codync: { latestRelease: { tagName: string } | null; releases: { nodes: Release[] } };
  tap: { cask: { text: string } | null };
};

export async function nudgeReleaseGate(env: Env): Promise<void> {
  if (!env.RELEASE_GATE_TOKEN) return;
  const headers = {
    Accept: "application/vnd.github+json",
    Authorization: `Bearer ${env.RELEASE_GATE_TOKEN}`,
    "User-Agent": "codync-cloud",
  };
  const res = await fetch("https://api.github.com/graphql", { method: "POST", headers, body: JSON.stringify({ query: STATE }) });
  const { data, errors } = (await res.json()) as { data?: State; errors?: unknown };
  if (!res.ok || !data) throw new Error(`GitHub graphql: ${res.status} ${JSON.stringify(errors)}`);
  const latest = data.codync.latestRelease?.tagName ?? "v0.0.0";
  const tap = /^\s+version "([^"]+)"/m.exec(data.tap.cask?.text ?? "")?.[1];
  if (!tap) throw new Error("the Homebrew cask has no version line");
  const waiting = data.codync.releases.nodes.filter((r) => !r.isDraft && !r.isPrerelease && isNewer(r.tagName, latest));
  if (waiting.length === 0 && tap === latest.replace(/^v/, "")) return;
  const dispatch = await fetch(DISPATCH, { method: "POST", headers, body: JSON.stringify({ ref: "main" }) });
  if (!dispatch.ok) throw new Error(`release gate dispatch: ${dispatch.status} ${await dispatch.text()}`);
  console.log("release gate run", { latest, waiting: waiting.map((r) => r.tagName), tap });
}
