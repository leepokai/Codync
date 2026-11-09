// App and host versions for Codync Cloud (docs/plans/codync-cloud.md, cloud soft and hard updates). Each app
// sends `Codync-App: <platform>/<version>; minHost=<version>`; the Worker keeps the latest per device, moves an
// account's cloud computer to the host its apps need, and refuses apps and self-hosted hosts below an
// emergency floor.

import type { ComputerRow } from "./routes/common";
import { ApiError } from "./routes/common";
import type { Env } from "./index";

export interface AppVersion {
  platform: "ios" | "android" | "desktop";
  version: string;
  minHost: string | null;
}

/** Apps not seen for this long don't hold an update back. */
const RECENT_MS = 30 * 86_400_000;
/** Rewrite an unchanged version at most this often (it marks the device as recently seen). */
const SEEN_EVERY_MS = 3600_000;
const VERSION = /^\d{1,4}\.\d{1,5}\.\d{1,6}$/;

/** `-1`, `0` or `1`, comparing x.y.z numerically. */
export function compareVersions(a: string, b: string): number {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) if (pa[i] !== pb[i]) return (pa[i] ?? 0) < (pb[i] ?? 0) ? -1 : 1;
  return 0;
}

const highest = (vs: (string | null | undefined)[]) =>
  vs.filter((v): v is string => !!v).reduce<string | null>((m, v) => (m === null || compareVersions(v, m) > 0 ? v : m), null);
const lowest = (vs: (string | null | undefined)[]) =>
  vs.filter((v): v is string => !!v).reduce<string | null>((m, v) => (m === null || compareVersions(v, m) < 0 ? v : m), null);

/** The `Codync-App` header; `null` when absent or malformed (an app from before this, or a bug: never blocks). */
export function parseApp(header: string | null): AppVersion | null {
  const m = /^(ios|android|desktop)\/([0-9.]+)(?:;\s*minHost=([0-9.]+))?$/.exec(header?.trim() ?? "");
  if (!m || !VERSION.test(m[2]!) || (m[3] && !VERSION.test(m[3]))) return null;
  return { platform: m[1] as AppVersion["platform"], version: m[2]!, minHost: m[3] ?? null };
}

/** Throws `426 appUpdateRequired` when an emergency floor (`APP_FLOOR_<PLATFORM>`) is above this app. */
export function checkAppFloor(env: Env, app: AppVersion | null): void {
  if (!app) return;
  const floor = { ios: env.APP_FLOOR_IOS, android: env.APP_FLOOR_ANDROID, desktop: env.APP_FLOOR_DESKTOP }[app.platform];
  if (floor && VERSION.test(floor) && compareVersions(app.version, floor) < 0) {
    throw new ApiError("appUpdateRequired", env.APP_FLOOR_REASON || "Update Codync to keep using it: this version has a security problem.", 426);
  }
}

/** Keeps what device `signPub` reported. */
export async function recordApp(env: Env, signPub: string, app: AppVersion | null, now: number): Promise<void> {
  if (!app) return;
  await env.DB.prepare(
    `UPDATE devices SET app_version = ?1, min_host = ?2, app_seen_at = ?3
     WHERE sign_pub = ?4 AND revoked_at IS NULL
       AND (app_version IS NOT ?1 OR min_host IS NOT ?2 OR app_seen_at IS NULL OR app_seen_at < ?5)`,
  )
    .bind(app.version, app.minHost, now, signPub, now - SEEN_EVERY_MS)
    .run();
}

interface Recent {
  oldestApp: string | null;
  neededHost: string | null;
}

/** The oldest app and the newest host the owner's recently used devices need. */
async function recent(env: Env, owner: string, now: number): Promise<Recent> {
  const { results } = await env.DB.prepare(
    "SELECT app_version, min_host FROM devices WHERE owner_user_id = ? AND revoked_at IS NULL AND app_seen_at > ?",
  )
    .bind(owner, now - RECENT_MS)
    .all<{ app_version: string | null; min_host: string | null }>();
  return { oldestApp: lowest(results.map((r) => r.app_version)), neededHost: highest(results.map((r) => r.min_host)) };
}

export interface UpdateOrder {
  version: string;
  force: boolean;
}

/**
 * A cloud computer's update that can't wait: an app of its owner needs a newer host (`minHost`), or an
 * emergency floor (`HOST_FLOOR`). Installed at once, also mid-task (bots' sessions resume). The target is the
 * oldest known release (`host_releases`) at or above the version needed, so older apps keep working: that version
 * itself, unless `dev` bumped past it before it was released. While no known release reaches it (the release gate
 * still holds it), the version needed as is.
 */
export async function urgentUpdate(env: Env, comp: ComputerRow, now: number): Promise<UpdateOrder | null> {
  if (!comp.owner_user_id) return null;
  const { neededHost } = await recent(env, comp.owner_user_id, now);
  const floor = env.HOST_FLOOR && VERSION.test(env.HOST_FLOOR) ? env.HOST_FLOOR : null;
  const needed = highest([neededHost, floor]);
  if (!needed || compareVersions(needed, comp.version) <= 0) return null;
  const { results } = await env.DB.prepare("SELECT version FROM host_releases").all<{ version: string }>();
  const published = lowest(results.map((r) => r.version).filter((v) => compareVersions(v, needed) >= 0));
  return { version: published ?? needed, force: true };
}

/**
 * The cloud soft update, offered when the host asks to sleep: the newest published release (up to its release
 * channel's latest) that every device of the last 30 days can use (`minApp` at most the oldest app) and that is at
 * least the host they need. Devices that never reported a version don't hold it back.
 */
export async function softUpdate(env: Env, comp: ComputerRow, now: number): Promise<UpdateOrder | null> {
  if (!comp.owner_user_id) return null;
  const { oldestApp, neededHost } = await recent(env, comp.owner_user_id, now);
  const { results } = await env.DB.prepare("SELECT version, min_app FROM host_releases").all<{ version: string; min_app: string }>();
  const fits = results.filter(
    (r) =>
      compareVersions(r.version, comp.version) > 0 &&
      (oldestApp === null || compareVersions(r.min_app, oldestApp) <= 0) &&
      (neededHost === null || compareVersions(r.version, neededHost) >= 0),
  );
  const target = highest(fits.map((r) => r.version));
  return target ? { version: target, force: false } : null;
}

/** A self-hosted computer below the emergency floor may not register; its host then updates itself. */
export function checkHostFloor(env: Env, version: string): void {
  const floor = env.HOST_FLOOR;
  if (floor && VERSION.test(floor) && VERSION.test(version) && compareVersions(version, floor) < 0) {
    throw new ApiError("hostUpdateRequired", env.HOST_FLOOR_REASON || "Update Codync on this computer: this version has a security problem.", 426);
  }
}
