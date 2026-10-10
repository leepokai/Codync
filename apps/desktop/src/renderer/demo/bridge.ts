import type { AccountState, CallError, CodyncBridge, HostSnapshot, UpdateState } from '@shared/ipc'
import { DEMO_UNAVAILABLE, FakeHost } from './fake-host'
import { COMPUTER_ID, HOST_NAME } from './story'
import { version } from '../../../package.json'

// `window.codync` for the browser: a running local host served by `FakeHost`, everything else
// signed out, unavailable or a no-op. Imported before the renderer so module-level reads see it.

const host = new FakeHost(version)
const none = () => () => {}
const unavailable = () => Promise.reject(new Error(DEMO_UNAVAILABLE))

const snapshot: HostSnapshot = {
  state: { kind: 'running' },
  local: { computerId: COMPUTER_ID, baseURL: 'http://demo.invalid', token: 'demo' },
  dev: false,
  logPath: '',
}
const account: AccountState = { ready: true, configured: false, busy: false, error: null, user: null, cloudURL: null }
const updates: UpdateState = {
  supported: false, canCheck: false, checking: false, availableVersion: null, staged: false,
  autoCheck: false, autoDownload: false, lastCheck: null, error: null, waitingForApp: null,
}

const bridge: CodyncBridge = {
  platform: 'darwin',
  appName: 'Codync',
  appScheme: 'codync',
  hostPort: 19222,
  appVersion: version,
  computerName: HOST_NAME,
  // Opens the group at launch, like the website's screenshot.
  debugOpen: 'Ship room',
  host: {
    snapshot: async () => snapshot,
    onChange: none,
    install() {},
    restart() {},
    uninstall() {},
    openLog() {},
    async call(_baseURL, _token, method, body) {
      await new Promise((r) => setTimeout(r, 30))
      try {
        return await host.call(method, (body ?? {}) as Record<string, unknown>)
      } catch (e) {
        throw (e as CallError).status ? e : ({ status: 500, message: String(e) } satisfies CallError)
      }
    },
    stream(url, _token, onData) {
      const since = Number(new URL(url).searchParams.get('since') ?? 0)
      return host.subscribe(since, onData)
    },
    health: async () => ({ computerId: COMPUTER_ID, busy: false }),
  },
  account: {
    state: async () => account,
    onChange: none,
    signIn: unavailable,
    signOut: async () => {},
    token: unavailable,
  },
  speech: { available: false, locales: () => [], start() {}, stop() {}, onEvent: none },
  ssh: {
    state: async () => ({ profiles: [], status: {}, attachments: [] }),
    onChange: none,
    save: async () => DEMO_UNAVAILABLE,
    remove() {},
    connect() {},
    disconnect() {},
    trustHostKey() {},
    chooseKey: async () => null,
  },
  cloud: { request: async () => ({ ok: false, error: { status: 503, code: 'demo', message: DEMO_UNAVAILABLE } }) },
  updates: { state: async () => updates, onChange: none, check() {}, setAutoCheck() {}, setAutoDownload() {} },
  files: { begin: unavailable, write: async () => {}, finish: async () => {}, cancel: async () => {} },
  app: {
    openExternal: (url) => void window.open(url, '_blank', 'noopener'),
    copy: (text) => void navigator.clipboard?.writeText(text).catch(() => {}),
    pickFiles: async () => [],
    readClipboardFiles: async () => [],
    setTraySummary() {},
    onCommand: none,
    openPairing() {},
    quit() {},
    resetAllData: async () => {},
    setLaunchAtLogin: async () => false,
    showInDock: async () => true,
    setShowInDock: async () => true,
    launchAtLogin: async () => false,
    openSettings() {},
    authenticate: unavailable,
    screenAgentState: async () => ({ available: false, needsApproval: false, registered: false }),
    setScreenAgent: unavailable,
    syncScreenAgent: async () => ({ needsApproval: false }),
  },
}

// Every visit starts from the story: no mirror cache or drafts from a previous visit, onboarding done,
// no GitHub star ask over the demo.
try {
  for (const key of Object.keys(localStorage)) if (key.startsWith('codync-')) localStorage.removeItem(key)
  localStorage.setItem('macAccountOnboardingCompleted', 'true')
  localStorage.setItem('computerAccessSetup', JSON.stringify('later'))
  localStorage.setItem('githubStarAsk', JSON.stringify({ kind: 'done' }))
} catch {}

// The dark palette everywhere (vite.demo.config.ts does the same for the CSS).
const matchMedia = window.matchMedia.bind(window)
window.matchMedia = (query) => matchMedia(query.includes('prefers-color-scheme: dark') ? 'all' : query)

window.codync = bridge
