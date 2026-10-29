import type { SignInFailure, SignInProvider } from './analytics'
import type { SharedFile } from './models'
import type { SSHBridge } from './ssh'
import type { ScreenAgentState } from './computer-access'
import type { IslandRect, IslandScreen, IslandSummary } from './island'
import type { VoiceKey, VoiceShortcut } from './voice-shortcut'
import type { ErrorReport } from './error-report'
import type { Appearance } from './appearance'

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

/** A menu image as PNG data URLs drawn for 1x and 2x screens; each screen shows its own. */
export interface TrayImage {
  x1: string
  x2: string
}

/** What the tray menu shows; the renderer owns the stores and sends this on every change. */
export interface TraySummary {
  status: string
  canPair: boolean
  bots: { ref: string; name: string; detail: string; working: boolean; avatar: TrayImage | null }[]
  approval: string | null
  usage: { name: string; icon: TrayImage | null; lines: { title: string; bar: TrayImage | null }[] }[]
  screen: {
    enabled: boolean
    subtitle: string
    error: string | null
  } | null
  /** This computer's networks (codync-network), lines already translated; null on hosts without them. */
  networks: { title: string; computers: { label: string; dot: TrayImage | null }[] }[] | null
  needsAttention: boolean
  textSize: number
  usageIconStyle: 'character' | 'original'
  /** The menu bar is a system menu, so the main process translates its labels from this. */
  language: 'en' | 'zh' | 'zh-Hant' | 'ja' | 'ko' | 'fr' | 'de' | 'es'
}

/** Things the tray menu (or the app menu) asks the window to do. */
export type WindowCommand =
  | { kind: 'openBot'; ref: string }
  | { kind: 'stopBot'; ref: string }
  | { kind: 'reviewApprovals' }
  | { kind: 'confirmReset' }
  | { kind: 'upToDate' }
  | { kind: 'setTextSize'; size: number }
  | { kind: 'stepTextSize'; up: boolean | null }
  | { kind: 'setUsageIconStyle'; style: 'character' | 'original' }
  | { kind: 'setRemoteScreen'; on: boolean }
  | { kind: 'computerAccess' }
  | { kind: 'networkSettings' }
  | { kind: 'newBot' }
  | { kind: 'search' }
  | { kind: 'toggleSidebar' }
  | { kind: 'toggleDetails' }
  | { kind: 'refresh' }
  | { kind: 'respondPermission'; ref: string; entryId: string; optionId: string }
  | { kind: 'sendMessage'; ref: string; text: string }
  /** The voice input shortcut, while the island is off: a double tap starts dictating into the composer, a tap sends. */
  | { kind: 'voiceKey'; key: VoiceKey }
  /** The chat window was shown again after being hidden (`window_opened`). */
  | { kind: 'windowOpened' }

/** The app's own updates (electron-updater, release builds only). */
export interface UpdateState {
  supported: boolean
  checking: boolean
  availableVersion: string | null
  /** Downloading: percent done (0–100); null when nothing is downloading. */
  downloadPercent: number | null
  /** Downloaded and ready: installs on restart. */
  staged: boolean
  /** Restarting into the staged release (the host stops first). */
  installing: boolean
  autoCheck: boolean
  autoDownload: boolean
  lastCheck: number | null
  error: string | null
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
  /** This launch was started by the login item (Open at login). */
  openedAtLogin: boolean
  /** Screenshot/UI checks: CODYNC_DEBUG_OPEN=newBot | newGroup | plugins | computers | <bot name>. */
  debugOpen: string | null
  host: {
    snapshot(): Promise<HostSnapshot>
    onChange(cb: (s: HostSnapshot) => void): () => void
    install(): void
    restart(): void
    /** POST /api/<method> on a loopback host. Rejects with a `CallError`. */
    call(baseURL: string, token: string, method: string, body: unknown, timeoutMs: number): Promise<unknown>
    /** SSE on a loopback host: each `data:` line, then `onEnd` (with an error when it failed). */
    stream(url: string, token: string, onData: (line: string) => void, onEnd: (error: CallError | null) => void): () => void
  }
  account: {
    state(): Promise<AccountState>
    onChange(cb: (s: AccountState) => void): () => void
    /** Why it ended without an account; null once signed in (or while another sign-in runs). */
    signIn(provider: SignInProvider): Promise<SignInFailure | null>
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
    /** The voice input shortcut (Settings → General); always `off` without on-device speech. */
    shortcut(): Promise<VoiceShortcut>
    setShortcut(shortcut: VoiceShortcut): Promise<VoiceShortcut>
    /** A call is on (or over): it has the microphone, so the voice input shortcut does nothing meanwhile. */
    setCalling(on: boolean): void
  }
  ssh: SSHBridge
  /** The notch island (shared/island.ts): its setting, the chat window's summary, and the island window's side. */
  island: {
    /** False where a window can't place itself at the top of the screen (Linux on Wayland). */
    supported: boolean
    enabled(): Promise<boolean>
    setEnabled(on: boolean): Promise<boolean>
    setSummary(summary: IslandSummary): void
    state(): Promise<{ screen: IslandScreen; summary: IslandSummary }>
    onSummary(cb: (summary: IslandSummary) => void): () => void
    onScreen(cb: (screen: IslandScreen) => void): () => void
    /** The pointer reached or left the shape. */
    onHover(cb: (inside: boolean) => void): () => void
    setShape(rect: IslandRect): void
    /** The message field takes the keyboard (the island never does otherwise). */
    setFocusable(on: boolean): void
    /** ⌥Y / ⌥N answer the approval on screen, from any app, while it shows. */
    setKeys(on: boolean): void
    onKey(cb: (answer: 'allow' | 'deny') => void): () => void
    /** The voice input shortcut's taps, from any app, while the island is on. */
    onVoiceKey(cb: (key: VoiceKey) => void): () => void
    respond(ref: string, entryId: string, optionId: string): void
    send(ref: string, text: string): void
    openBot(ref: string): void
    newBot(): void
    openApp(): void
  }
  /** The Codync cloud's `/v1` API with the signed-in session (and the device key for `signed`). */
  cloud: {
    request(method: string, path: string, body: unknown, signed: boolean): Promise<{ ok: true; value: unknown } | { ok: false; error: { status: number; code: string; message: string | null } }>
  }
  updates: {
    state(): Promise<UpdateState>
    onChange(cb: (s: UpdateState) => void): () => void
    check(): void
    /** Downloads the available release if needed, then restarts into it. */
    install(): void
    setAutoCheck(on: boolean): void
    setAutoDownload(on: boolean): void
  }
  /** Artifacts shown in a frame of their own (`main/artifacts.ts`). */
  artifacts: {
    /** Makes `codync-artifact://<id>` serve this HTML. */
    show(id: string, html: Uint8Array): Promise<void>
    openInBrowser(id: string, html: Uint8Array): Promise<void>
  }
  files: {
    begin(file: SharedFile): Promise<string | null>
    write(id: string, offset: number, data: Uint8Array): Promise<void>
    finish(id: string): Promise<void>
    cancel(id: string): Promise<void>
  }
  app: {
    /** Error tracking: an uncaught error in this window (shared/error-report.ts). */
    reportError(report: ErrorReport): void
    openExternal(url: string): void
    copy(text: string): void
    /** Files picked in the system dialog: name and bytes. */
    pickFiles(): Promise<{ name: string; data: Uint8Array }[]>
    readClipboardFiles(): Promise<{ name: string; data: Uint8Array }[]>
    setTraySummary(summary: TraySummary): void
    onCommand(cb: (command: WindowCommand) => void): () => void
    resetAllData(): Promise<void>
    setLaunchAtLogin(on: boolean): Promise<boolean>
    appearance(): Promise<Appearance>
    setAppearance(value: Appearance): Promise<Appearance>
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
