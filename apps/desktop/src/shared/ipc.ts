import type { SharedFile } from './models'
import type { SSHBridge } from './ssh'
import type { ScreenAgentState } from './computer-access'

// The bridge between the main process (host lifecycle, loopback HTTP, tray, windows)
// and the renderer (stores and UI). Exposed as `window.codync` by the preload script.

export type HostState =
  | { kind: 'missingBinary' }
  | { kind: 'notInstalled' }
  | { kind: 'starting' }
  | { kind: 'running' }
  | { kind: 'failed'; message: string }

/** This computer's own host, once it answered. */
export interface LocalHost {
  computerId: string
  baseURL: string
  token: string
}

export interface HostSnapshot {
  state: HostState
  local: LocalHost | null
  /** Started against a manually run host (`CODYNC_PORT`). */
  dev: boolean
  logPath: string
}

export interface CallError {
  status: number
  message: string
}

/** What the tray menu shows; the renderer owns the stores and sends this on every change. */
export interface TraySummary {
  status: string
  canPair: boolean
  bots: { ref: string; name: string; detail: string; working: boolean; avatar: string | null }[]
  approval: string | null
  usage: { name: string; icon: string | null; lines: { title: string; bar: string | null }[] }[]
  screen: {
    enabled: boolean
    subtitle: string
    needsLoginItem: boolean
    needsCapture: boolean
    needsInput: boolean
    error: string | null
  } | null
  version: string | null
  needsAttention: boolean
  textSize: number
  usageIconStyle: 'character' | 'original'
}

/** Things the tray menu (or the app menu) asks the window to do. */
export type WindowCommand =
  | { kind: 'openBot'; ref: string }
  | { kind: 'stopBot'; ref: string }
  | { kind: 'reviewApprovals' }
  | { kind: 'confirmReset' }
  | { kind: 'setTextSize'; size: number }
  | { kind: 'stepTextSize'; up: boolean | null }
  | { kind: 'setUsageIconStyle'; style: 'character' | 'original' }
  | { kind: 'setRemoteScreen'; on: boolean }
  | { kind: 'computerAccess' }
  | { kind: 'newChat' }
  | { kind: 'search' }
  | { kind: 'toggleSidebar' }
  | { kind: 'toggleDetails' }

/** The app's own updates (electron-updater, release builds only). */
export interface UpdateState {
  supported: boolean
  canCheck: boolean
  checking: boolean
  availableVersion: string | null
  /** Downloaded and ready: installs on restart. */
  staged: boolean
  autoCheck: boolean
  autoDownload: boolean
  lastCheck: number | null
  error: string | null
  /** The release waits until paired iPhones can get the app it needs. */
  waitingForApp: string | null
}

export interface AccountState {
  /** The persisted session has been restored (or there was none). */
  ready: boolean
  /** This build has a Clerk key and a cloud. */
  configured: boolean
  busy: boolean
  error: string | null
  user: { userId: string; email: string | null; avatarURL: string | null } | null
  cloudURL: string | null
}

export type SpeechEvent =
  | { type: 'partial' | 'final'; text: string }
  | { type: 'level'; value: number }
  | { type: 'started' }
  | { type: 'stopped' }
  | { type: 'error'; message: string }

export interface CodyncBridge {
  platform: 'darwin' | 'linux' | 'win32'
  appName: string
  appScheme: string
  hostPort: number
  appVersion: string
  /** The computer's name ("Kevin's MacBook Pro"). */
  computerName: string
  /** Screenshot/UI checks: CODYNC_DEBUG_OPEN=compose | group | plugins | computers | <bot name>. */
  debugOpen: string | null
  host: {
    snapshot(): Promise<HostSnapshot>
    onChange(cb: (s: HostSnapshot) => void): () => void
    install(): void
    restart(): void
    uninstall(): void
    openLog(): void
    /** POST /api/<method> on a loopback host. Rejects with a `CallError`. */
    call(baseURL: string, token: string, method: string, body: unknown, timeoutMs: number): Promise<unknown>
    /** SSE on a loopback host: each `data:` line, then `onEnd` (with an error when it failed). */
    stream(url: string, token: string, onData: (line: string) => void, onEnd: (error: CallError | null) => void): () => void
    health(baseURL: string): Promise<{ computerId?: string; busy?: boolean } | null>
  }
  account: {
    state(): Promise<AccountState>
    onChange(cb: (s: AccountState) => void): () => void
    signIn(provider: 'apple' | 'google'): Promise<void>
    signOut(): Promise<void>
    /** The signed-in session's JWT for the Codync cloud. */
    token(): Promise<string>
  }
  /** On-device speech recognition (macOS: SFSpeechRecognizer in a helper; unavailable on Linux). */
  speech: {
    available: boolean
    /** The languages it can transcribe, as locale identifiers. */
    locales(): string[]
    /** `locale` null: the computer's language. */
    start(locale: string | null): void
    stop(): void
    onEvent(cb: (e: SpeechEvent) => void): () => void
  }
  ssh: SSHBridge
  /** The Codync cloud's `/v1` API with the signed-in session (and the device key for `signed`). */
  cloud: {
    request(method: string, path: string, body: unknown, signed: boolean): Promise<{ ok: true; value: unknown } | { ok: false; error: { status: number; code: string; message: string | null } }>
  }
  updates: {
    state(): Promise<UpdateState>
    onChange(cb: (s: UpdateState) => void): () => void
    check(): void
    setAutoCheck(on: boolean): void
    setAutoDownload(on: boolean): void
  }
  files: {
    begin(file: SharedFile): Promise<string | null>
    write(id: string, offset: number, data: Uint8Array): Promise<void>
    finish(id: string): Promise<void>
    cancel(id: string): Promise<void>
  }
  app: {
    openExternal(url: string): void
    copy(text: string): void
    /** Files picked in the system dialog: name and bytes. */
    pickFiles(): Promise<{ name: string; data: Uint8Array }[]>
    readClipboardFiles(): Promise<{ name: string; data: Uint8Array }[]>
    setTraySummary(summary: TraySummary): void
    onCommand(cb: (command: WindowCommand) => void): () => void
    openPairing(): void
    quit(): void
    resetAllData(): Promise<void>
    setLaunchAtLogin(on: boolean): Promise<boolean>
    showInDock(): Promise<boolean>
    setShowInDock(on: boolean): Promise<boolean>
    launchAtLogin(): Promise<boolean>
    openSettings(url: string): void
    /**
     * Opens `url` in the system browser and resolves with the URL it returns to on `scheme`
     * (`codync://…`), like ASWebAuthenticationSession. Rejects when cancelled or timed out.
     */
    authenticate(url: string, scheme: string): Promise<string>
    /** macOS: registers or removes the Codync Screen agent (SMAppService). */
    screenAgentState(): Promise<ScreenAgentState>
    setScreenAgent(on: boolean): Promise<{ needsApproval: boolean }>
    syncScreenAgent(enabled: boolean): Promise<{ needsApproval: boolean }>
  }
}
