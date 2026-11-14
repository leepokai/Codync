// Milestones in #milestones, each posted once (no pings) with the way left to the goal: every 100th GitHub
// star (from the repo webhook) and every 1,000th download of the desktop app (counted once a day).

import { githubAll, send } from "./guild.ts";
import type { Env } from "./index.ts";

export type Kind = "stars" | "downloads";

const KINDS: Record<Kind, { step: number; goal: (env: Env) => string; emoji: string; label: string; thanks: (repo: string) => string }> = {
  stars: {
    step: 100,
    goal: (env) => env.STAR_GOAL,
    emoji: "⭐",
    label: "stars on GitHub",
    thanks: (repo) => `Thanks to everyone who starred [${repo}](https://github.com/${repo}) 💛`,
  },
  downloads: {
    step: 1000,
    goal: (env) => env.DOWNLOAD_GOAL,
    emoji: "📦",
    label: "downloads",
    thanks: () => "Thanks to everyone who gave it a try 💛",
  },
};
const BAR = 20;

/** Posts the milestone the count has reached, once: a count that dips and climbs back (an unstar) doesn't repeat it. */
export async function celebrate(env: Env, kind: Kind, count: number): Promise<void> {
  const { step } = KINDS[kind];
  const milestone = Math.floor(count / step) * step;
  const key = `${kind}:milestone`;
  const last = Number((await env.LINKS.get(key)) ?? 0);
  if (milestone <= last) return;
  // ponytail: KV isn't atomic, so two stars delivered at the same instant on a new milestone could post it twice.
  if (await send(env, env.MILESTONES_CHANNEL_ID, milestoneMessage(kind, milestone, Number(KINDS[kind].goal(env)), env.REPO))) {
    await env.LINKS.put(key, String(milestone));
    console.log("milestone", { kind, milestone });
  }
}

// Installers people download from the site or the releases page; updater files, checksums and host tarballs
// aren't someone new trying Codync.
const INSTALLER = /\.(dmg|exe|appimage|deb|rpm)$|^codync-linux-.*\.tar\.gz$/i;

/** Desktop app downloads across every release. */
export async function downloads(env: Env): Promise<number> {
  const releases = await githubAll<{ assets: { name: string; download_count: number }[] }>(env, "releases");
  return releases.flatMap((r) => r.assets).filter((a) => INSTALLER.test(a.name)).reduce((sum, a) => sum + a.download_count, 0);
}

const PITCH = "Codync: your coding agents, as teammates you can message. Free and open source.";

/** One-click posts about Codync on the usual social platforms, linking the repo. */
function shareLinks(url: string): string {
  const q = (params: Record<string, string>) => new URLSearchParams(params).toString().replaceAll("+", "%20");
  return [
    `[X](https://x.com/intent/post?${q({ text: PITCH, url })})`,
    `[Threads](https://www.threads.net/intent/post?${q({ text: `${PITCH} ${url}` })})`,
    `[Reddit](https://www.reddit.com/submit?${q({ url, title: PITCH })})`,
    `[LinkedIn](https://www.linkedin.com/sharing/share-offsite/?${q({ url })})`,
  ].join(" · ");
}

export function milestoneMessage(kind: Kind, count: number, goal: number, repo: string): string {
  const { step, emoji, label, thanks } = KINDS[kind];
  const n = (x: number) => x.toLocaleString("en-US");
  const head = count >= goal && count - step < goal
    ? `🎉 **Codync hit ${n(count)} ${label}, goal reached!**`
    : `${emoji} **Codync just hit ${n(count)} ${label}!**`;
  const filled = Math.round((count / goal) * BAR);
  const progress = count < goal ? `\n\`${"█".repeat(filled)}${"░".repeat(BAR - filled)}\` ${n(count)} / ${n(goal)}` : "";
  const ask = count < goal ? `Help us reach ${n(goal)}: tell people about Codync` : "Keep spreading the word";
  return `${head}${progress}\n${thanks(repo)}\n\n📣 ${ask} on ${shareLinks(`https://github.com/${repo}`)}, or wherever you hang out.`;
}
