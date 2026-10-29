import { randomUUID } from 'node:crypto'
import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { app } from 'electron'
import { maySendSetupFailure } from '../shared/analytics'
import { identity } from './environment'
import { baseURL, dataDir, prefs } from './host-controller'

// `host_setup_failed`: the one event the desktop app sends to PostHog itself, since without a
// working host there's nothing to report through. Everything else goes through the host
// (docs/features/analytics.md).

/** Where getting a working host failed. */
export type HostSetupStage = 'missing_binary' | 'install' | 'verify' | 'not_responding' | 'environment'

/** Write-only project token, the host's (host/src/analytics.rs): safe to ship in public builds. */
const TOKEN = 'phc_rQHvxdSFKnMquPfMjmiMhXhM4tqJ5qCYsWuVEoFxCCWX'
const BATCH_URL = 'https://us.i.posthog.com/batch/'
/** The host's `$os` names. */
const OS: Partial<Record<NodeJS.Platform, string>> = { darwin: 'macos', win32: 'windows', linux: 'linux' }
const ENABLED = 'analyticsEnabled'
const INSTALL_ID = 'analyticsInstallId'

const reported = new Set<HostSetupStage>()

/** The computer owner's choice, as its host last answered it (`hello`, `setAnalytics`); unknown until then. */
export function noteAnalyticsSetting(url: string, method: string, value: unknown) {
  // Not an SSH computer's host: those reach the window through loopback tunnels too.
  if (url !== baseURL) return
  const answer = value as { analytics?: unknown; enabled?: unknown } | null
  let on: unknown
  if (method === 'hello') on = answer?.analytics
  else if (method === 'setAnalytics') on = answer?.enabled
  if (typeof on === 'boolean' && prefs.get(ENABLED) !== on) prefs.set(ENABLED, on)
}

/** Once per stage per launch; never from a development run, nor when the owner may have turned analytics off. */
export function reportHostSetupFailure(stage: HostSetupStage) {
  if (!app.isPackaged || reported.has(stage)) return
  if (!maySendSetupFailure(prefs.get(ENABLED), existsSync(join(dataDir, 'codync.db')))) return
  reported.add(stage)
  const event = {
    event: 'host_setup_failed',
    distinct_id: installId(),
    timestamp: new Date().toISOString(),
    uuid: randomUUID(),
    properties: {
      $lib: 'codync-desktop',
      app_version: app.getVersion(),
      env: identity.environment,
      $os: OS[process.platform] ?? process.platform,
      $process_person_profile: true,
      stage,
    },
  }
  void fetch(BATCH_URL, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ api_key: TOKEN, batch: [event] }),
    signal: AbortSignal.timeout(10_000),
  }).then(
    (res) => { if (!res.ok) console.warn(`host_setup_failed: PostHog answered ${res.status}`) },
    (error: unknown) => console.warn('host_setup_failed not sent', error),
  )
}

/** A random id for this install, kept in the app's preferences. */
function installId() {
  const saved = prefs.get(INSTALL_ID)
  if (typeof saved === 'string') return saved
  const id = randomUUID()
  prefs.set(INSTALL_ID, id)
  return id
}
