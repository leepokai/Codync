import type { AccountState, CallError, CodyncBridge, HostSnapshot, UpdateState } from '@shared/ipc'
import type { SSHState } from '@shared/ssh'
import { DEMO_UNAVAILABLE, FakeHost } from './fake-host'
import { macBook, macMini } from './story'
import { version } from '../../../package.json'

// `window.codync` for the browser: this Mac's running host and a Mac mini connected over SSH, each
// served by a `FakeHost` at its own base URL; everything else signed out, unavailable or a no-op.
// Imported before the renderer so module-level reads see it.

const LOCAL_URL = 'http://demo.invalid'
const MINI_URL = 'http://mini.demo.invalid'
const hosts = new Map([[LOCAL_URL, new FakeHost(version, macBook)], [MINI_URL, new FakeHost(version, macMini)]])
const hostAt = (url: string) => hosts.get(new URL(url).origin)!
const none = () => () => {}
const unavailable = () => Promise.reject(new Error(DEMO_UNAVAILABLE))

const snapshot: HostSnapshot = {
  state: { kind: 'running' },
  local: { computerId: macBook.id, baseURL: LOCAL_URL, token: 'demo' },
  dev: false,
  logPath: '',
}
const account: AccountState = { ready: true, configured: false, busy: false, error: null, user: null, cloudURL: null }
const sshState: SSHState = {
  profiles: [{ id: 'mini', host: 'mac-mini', port: null, user: null, identityFile: null, remotePort: 19222, computerId: macMini.id, name: macMini.name }],
  status: { mini: { kind: 'connected', localPort: 19223 } },
  attachments: [{ profileId: 'mini', computer: { id: macMini.id, name: macMini.name, signKey: '', boxKey: null }, baseURL: MINI_URL, token: 'demo' }],
  needsLinger: [],
}
const updates: UpdateState = {
  supported: false, checking: false, availableVersion: null, downloadPercent: null, staged: false, installing: false,
  autoCheck: false, autoDownload: false, lastCheck: null, error: null,
}

const bridge: CodyncBridge = {
  platform: 'darwin',
  appName: 'Codync',
  appScheme: 'codync',
  hostPort: 19222,
  appVersion: version,
  computerName: macBook.name,
  openedAtLogin: false,
  // Opens the group at launch, like the website's screenshot.
  debugOpen: 'Ship room',
  host: {
    snapshot: async () => snapshot,
    onChange: none,
    install() {},
    restart() {},
    async call(baseURL, _token, method, body) {
      await new Promise((r) => setTimeout(r, 30))
      try {
        return await hostAt(baseURL).call(method, (body ?? {}) as Record<string, unknown>)
      } catch (e) {
        throw (e as CallError).status ? e : ({ status: 500, message: String(e) } satisfies CallError)
      }
    },
    stream(url, _token, onData) {
      const since = Number(new URL(url).searchParams.get('since') ?? 0)
      return hostAt(url).subscribe(since, onData)
    },
  },
  channel: {
    call: async () => { throw { status: 503, message: DEMO_UNAVAILABLE } },
    stream: (_c, _k, _p, _d, onEnd) => {
      onEnd({ status: 503, message: DEMO_UNAVAILABLE })
      return () => {}
    },
  },
  account: {
    state: async () => account,
    onChange: none,
    signIn: unavailable,
    signOut: async () => {},
    deleteAccount: async () => false,
    token: unavailable,
  },
  speech: { available: false, locales: () => [], start() {}, stop() {}, onEvent: none, shortcut: async () => 'off', setShortcut: async () => 'off', setCalling() {} },
  ssh: {
    state: async () => sshState,
    onChange: none,
    save: async () => DEMO_UNAVAILABLE,
    remove() {},
    connect() {},
    disconnect() {},
    trustHostKey() {},
    install() {},
    chooseKey: async () => null,
    configHosts: async () => ({ access: 'denied', hosts: [] }),
    setConfigAccess: async () => ({ access: 'denied', hosts: [] }),
  },
  island: {
    supported: false,
    enabled: async () => false,
    setEnabled: async () => false,
    setSummary() {},
    state: async () => ({ screen: { notch: null }, summary: { bots: [], permissions: [], usage: [] } }),
    onSummary: none,
    onScreen: none,
    onHover: none,
    setShape() {},
    setFocusable() {},
    setKeys() {},
    onKey: none,
    onVoiceKey: none,
    respond() {},
    send() {},
    openBot() {},
    newBot() {},
    openApp() {},
  },
  cloud: {
    request: async () => ({ ok: false, error: { status: 503, code: 'demo', message: DEMO_UNAVAILABLE } }),
    requestAccess: async () => ({ ok: false, error: { status: 503, code: 'demo', message: DEMO_UNAVAILABLE } }),
  },
  updates: { state: async () => updates, onChange: none, check() {}, install() {}, setAutoCheck() {}, setAutoDownload() {} },
  artifacts: { show: async () => {}, openInBrowser: unavailable },
  files: { begin: unavailable, write: async () => {}, finish: async () => {}, cancel: async () => {} },
  app: {
    attended: async () => true,
    onAttended: none,
    reportError() {},
    openExternal: (url) => void window.open(url, '_blank', 'noopener'),
    copy: (text) => void navigator.clipboard?.writeText(text).catch(() => {}),
    pickFiles: async () => [],
    pickExport: async () => null,
    readClipboardFiles: async () => [],
    setTraySummary() {},
    showWindow() {},
    onCommand: none,
    resetAllData: async () => {},
    setLaunchAtLogin: async () => false,
    appearance: async () => 'system',
    setAppearance: async (value) => value,
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

// Every visit starts from the story: no mirror cache, drafts or computer filter, order or folding from a
// previous visit, onboarding done, no GitHub star ask, Ko-fi invitation or tip about Extensions over the demo.
try {
  for (const key of Object.keys(localStorage)) if (key.startsWith('codync-')) localStorage.removeItem(key)
  for (const key of ['hiddenComputers', 'computerOrder', 'collapsedComputerSections']) localStorage.removeItem(key)
  localStorage.setItem('macAccountOnboardingCompleted', 'true')
  localStorage.setItem('computerAccessSetup', JSON.stringify('later'))
  localStorage.setItem('githubStarAsk', JSON.stringify({ kind: 'done' }))
  localStorage.setItem('kofiInvite', JSON.stringify({ kind: 'done' }))
  localStorage.setItem('extensionsMovedTipSeen', 'true')
} catch {}

// The dark palette everywhere (vite.demo.config.ts does the same for the CSS).
const matchMedia = window.matchMedia.bind(window)
window.matchMedia = (query) => matchMedia(query.includes('prefers-color-scheme: dark') ? 'all' : query)

window.codync = bridge
