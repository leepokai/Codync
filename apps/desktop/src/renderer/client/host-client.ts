import type { CallError, RelayAbsence, RelayPresence } from '@shared/ipc'
import {
  normalizeBot,
  normalizeScreen,
  type AccessRequest,
  type Bot,
  type CloudStatus,
  type Entry,
  type Hello,
  type PairingRequest,
  type ReplyQuote,
  type ScreenState,
  type Usage,
} from '@shared/models'
import { parseAlert, type HostAlert } from '@shared/alerts'
import type { AgentModels } from '@shared/agent-models'
import type { Artifact, ArtifactChunk, LibraryItem } from '@shared/artifacts'
import { parseCallCue, type CallCue } from '@shared/calls'
import type { NetworkView } from '@shared/network'
import { fromOldAnswer, fromOldEvent, toHost } from '@shared/old-host'
import { parseThreadEntry } from '@shared/threads'
import { trNow } from '../lib/l10n.ts'

/** How a client reaches a host API: loopback (the computer's own host or an SSH tunnel) or the relay (a cloud computer). */
export type HostRoute = 'loopback' | 'direct' | 'relay'

export class HostError extends Error {
  constructor(
    readonly status: number,
    message: string,
    /** The computer isn't connected to the relay (`RelayPresence`): asleep (the connection wakes a cloud computer) or
     * off, or a cloud computer the relay is waking. */
    readonly presence: RelayAbsence | null = null,
  ) {
    super(message)
  }

  static unreachable() {
    return new HostError(0, trNow("Can't reach your computer. Is it on and connected to the internet?"))
  }

  /** The link failed, not the request: the same call can work once the computer is reachable again. */
  get isTransient() {
    return this.status === 0
  }
}

export interface HostTransport {
  /** How it reaches the host, shown in the chat header. */
  readonly route?: HostRoute
  call(method: string, body: unknown, timeoutMs: number): Promise<unknown>
  /** Each element is one event's JSON (what SSE puts after `data:`). */
  stream(path: string, onData: (line: string) => void, onEnd: (error: HostError | null) => void): () => void
  /** Each presence the relay tells of the computer, through the relay only. */
  onPresence?(cb: (presence: RelayPresence) => void): () => void
}

export class LoopbackTransport implements HostTransport {
  readonly route = 'loopback'

  constructor(
    readonly baseURL: string,
    private token: string,
  ) {}

  async call(method: string, body: unknown, timeoutMs: number) {
    try {
      return await window.codync.host.call(this.baseURL, this.token, method, body, timeoutMs)
    } catch (e) {
      const error = e as CallError
      throw new HostError(error.status ?? 0, error.status === 401 ? trNow("This device isn't allowed on that computer anymore.") : error.message)
    }
  }

  stream(path: string, onData: (line: string) => void, onEnd: (error: HostError | null) => void) {
    return window.codync.host.stream(`${this.baseURL}${path}`, this.token, onData, (error) =>
      onEnd(error ? new HostError(error.status, error.message) : null),
    )
  }
}

/** An account computer through the relay's encrypted channel, which the main process keeps (`main/channel`). */
export class ChannelTransport implements HostTransport {
  readonly route = 'relay'

  constructor(
    readonly computerId: string,
    private signKey: string,
  ) {}

  async call(method: string, body: unknown, timeoutMs: number) {
    try {
      return await window.codync.channel.call(this.computerId, this.signKey, method, body, timeoutMs)
    } catch (e) {
      throw channelError(e as CallError)
    }
  }

  stream(path: string, onData: (line: string) => void, onEnd: (error: HostError | null) => void) {
    return window.codync.channel.stream(this.computerId, this.signKey, path, onData, (error) => onEnd(error ? channelError(error) : null))
  }

  onPresence(cb: (presence: RelayPresence) => void) {
    return window.codync.channel.onPresence((p) => p.computerId === this.computerId && cb(p.presence))
  }
}

const channelError = (e: CallError) => new HostError(e.status ?? 0, e.message, e.presence ?? null)

export type HostEvent =
  | { type: 'icons'; iconsRevision: number; icons: import('@shared/custom-icons').CustomIcons }
  /** `network` is null from hosts before the network view. `catchUp` events follow before the live ones (null from hosts before 2.32.0). `workflows` is absent on older hosts. */
  | { type: 'hello'; hostId: string; rev: number; usage: Usage; screen: ScreenState | null; network: NetworkView | null; icons?: import('@shared/custom-icons').CustomIcons; catchUp: number | null; workflows?: boolean; scheduleSend?: boolean; held?: Entry[] }
  | { type: 'bot'; bot: Bot }
  /** Live only: a bot is done, needs you or failed (never replayed in a catch-up). */
  | { type: 'alert'; alert: HostAlert }
  | { type: 'botDeleted'; id: string; rev: number }
  | { type: 'entry'; entry: Entry }
  /** One entry of a thread's transcript (never a chat's); the catch-up has none. */
  | { type: 'threadEntry'; thread: string; entry: Entry }
  | { type: 'usage'; usage: Usage }
  /** An agent's model list the computer just read (`loginProfile` null: the default login); no rev. */
  | { type: 'models'; backend: string; loginProfile: string | null; models: AgentModels }
  /** An artifact was published, updated or renamed (the catch-up replays it too). */
  | { type: 'artifact'; artifact: Artifact }
  | { type: 'screen'; screen: ScreenState }
  | { type: 'accessRequests'; requests: AccessRequest[] }
  | { type: 'pairingRequests'; requests: PairingRequest[] }
  | { type: 'cloud'; cloud: CloudStatus }
  /** The computer took a new name (a cloud computer renamed from any device): its `hello` says it from now on. */
  | { type: 'name'; name: string }
  /** The computer's automatic bot order was switched (from any device): its `hello` says it from now on. */
  | { type: 'autoOrder'; enabled: boolean }
  /** The computer's choice of when old threads are deleted changed (from any device): its `hello` says it from now on. */
  | { type: 'threadKeepDays'; days: number }
  /** This computer's network changed (members, roles, presence, offered bots) or cloud assistants were switched. */
  | { type: 'network'; network: NetworkView }
  /** For the voice of a call in this app (only the computer's own apps receive this computer's calls' cues). */
  | { type: 'call'; cue: CallCue }
  | { type: 'resync' }
  /** One this app has no use for. */
  | { type: 'skipped' }
  /** A workflow change. It carries no `rev` and never means the event stream is unreadable. */
  | { type: 'workflow'; change: 'graph' | 'enabled' | 'run' | 'step' | 'deleted'; workflowId: string; runId: string | null; stepId?: string; enabled?: boolean }
  /** An event this app version couldn't read (host newer/older than the app). */
  | { type: 'undecodable'; kind: string }

export function parseEvent(line: string): HostEvent | null {
  let raw: Record<string, unknown>
  try {
    // Hosts before 2.51.0: goes when MIN_HOST reaches 2.51.0.
    raw = fromOldEvent(JSON.parse(line) as Record<string, unknown>)
  } catch {
    return null
  }
  const type = raw.type
  switch (type) {
    case 'icons':
      return { type, iconsRevision: Number(raw.iconsRevision ?? 0), icons: (raw.icons ?? {}) as import('@shared/custom-icons').CustomIcons }
    case 'hello':
      return {
        type,
        hostId: (raw.hostId as string) ?? '',
        rev: (raw.rev as number) ?? 0,
        usage: (raw.usage as Usage) ?? { providers: [] },
        screen: normalizeScreen(raw.screen as ScreenState | undefined),
        network: (raw.network as NetworkView | undefined) ?? null,
        workflows: raw.workflows === true,
        // A hello from an older host omits both: left undefined so the value already known stands.
        scheduleSend: typeof raw.scheduleSend === 'boolean' ? raw.scheduleSend : undefined,
        held: Array.isArray(raw.held) ? (raw.held as Entry[]) : undefined,
        icons: raw.icons as import('@shared/custom-icons').CustomIcons | undefined,
        catchUp: typeof raw.catchUp === 'number' ? raw.catchUp : null,
      }
    case 'bot': {
      const bot = raw.bot as (Bot & { deleted?: boolean }) | undefined
      if (!bot || typeof bot.id !== 'string') return { type: 'undecodable', kind: 'bot' }
      if (bot.deleted) return { type: 'botDeleted', id: bot.id, rev: bot.rev ?? 0 }
      return { type, bot: normalizeBot(bot) }
    }
    case 'entry': {
      const entry = raw.entry as Entry | undefined
      if (!entry || typeof entry.id !== 'string' || typeof entry.botId !== 'string') return { type: 'undecodable', kind: 'entry' }
      return { type, entry: { ...entry, data: entry.data ?? {} } }
    }
    case 'threadEntry': {
      const parsed = parseThreadEntry(raw)
      return parsed ? { type, ...parsed } : null
    }
    case 'usage':
      return raw.usage ? { type, usage: raw.usage as Usage } : null
    case 'models': {
      const models = raw.models as AgentModels | undefined
      if (typeof raw.backend !== 'string' || !Array.isArray(models?.models)) return null
      return { type, backend: raw.backend, loginProfile: typeof raw.loginProfile === 'string' ? raw.loginProfile : null, models }
    }
    case 'artifact': {
      const artifact = raw.artifact as Artifact | undefined
      return artifact && typeof artifact.id === 'string' && typeof artifact.botId === 'string' ? { type, artifact } : null
    }
    case 'screen': {
      const screen = normalizeScreen(raw.screen as ScreenState | undefined)
      return screen ? { type, screen } : null
    }
    case 'accessRequests':
      return { type, requests: (raw.requests as AccessRequest[]) ?? [] }
    case 'pairingRequests':
      return { type, requests: (raw.requests as PairingRequest[]) ?? [] }
    case 'cloud':
      return raw.cloud ? { type, cloud: raw.cloud as CloudStatus } : null
    case 'name':
      return typeof raw.name === 'string' ? { type, name: raw.name } : null
    case 'autoOrder':
      return typeof raw.enabled === 'boolean' ? { type, enabled: raw.enabled } : null
    case 'threadKeepDays':
      return typeof raw.days === 'number' ? { type, days: raw.days } : null
    case 'network':
      return raw.network ? { type, network: raw.network as NetworkView } : null
    case 'call': {
      const cue = parseCallCue(raw)
      return cue ? { type, cue } : null
    }
    case 'alert': {
      const alert = parseAlert(raw.alert)
      return alert ? { type, alert } : null
    }
    case 'resync':
      return { type }
    case 'workflow': {
      const change = raw.change
      const workflowId = raw.workflowId
      if (typeof workflowId !== 'string') return null
      if (change !== 'graph' && change !== 'enabled' && change !== 'run' && change !== 'step' && change !== 'deleted') return null
      return {
        type,
        change,
        workflowId,
        runId: typeof raw.runId === 'string' ? raw.runId : null,
        stepId: typeof raw.stepId === 'string' ? raw.stepId : undefined,
        enabled: typeof raw.enabled === 'boolean' ? raw.enabled : undefined,
      }
    }
    default:
      return null
  }
}

/** Typed host calls (host/src/api/); anything else goes through `call`. */
export class HostClient {
  readFile(entryId: string, fileId: string, offset: number): Promise<{ data: string; size: number }> {
    return this.call('readFile', { entryId, fileId, offset })
  }

  library(botId: string): Promise<{ items: LibraryItem[] }> {
    return this.call('library', { botId })
  }

  readArtifact(botId: string, artifactId: string, offset: number): Promise<ArtifactChunk> {
    return this.call('readArtifact', { botId, artifactId, offset })
  }

  renameArtifact(botId: string, artifactId: string, title: string): Promise<{ artifact: Artifact }> {
    return this.call('renameArtifact', { botId, artifactId, title })
  }

  /** `hostVersion`: the host's version, once `hello` said it; a new connection to the same computer starts from the last one's. */
  constructor(readonly transport: HostTransport, private hostVersion?: string) {}

  call<T>(method: string, body: unknown = {}, timeoutMs = 20_000): Promise<T> {
    // Hosts before 2.51.0: goes when MIN_HOST reaches 2.51.0.
    const [sent, sentBody] = toHost(this.hostVersion, method, body)
    return this.transport.call(sent, sentBody, timeoutMs).then((answer) => fromOldAnswer(method, answer)) as Promise<T>
  }

  /** Catch-up since `since`, then live updates. */
  events(since: number, client: string, onEvent: (e: HostEvent) => void, onEnd: (error: HostError | null) => void) {
    const query = new URLSearchParams({ since: String(since), client })
    return this.transport.stream(`/events?${query}`, (line) => {
      // One this app has no use for still counts toward the catch-up `hello` announced.
      onEvent(parseEvent(line) ?? { type: 'skipped' })
    }, onEnd)
  }

  hello() {
    return this.call<Hello>('hello').then((h) => {
      this.hostVersion = h.version
      return {
        ...h,
        backends: h.backends ?? [],
        screen: normalizeScreen(h.screen) ?? undefined,
        workflows: h.workflows === true,
        scheduleSend: h.scheduleSend === true,
        held: h.held ?? [],
      }
    })
  }

  async history(botId: string, beforeSeq: number, limit = 100) {
    return (await this.call<{ entries: Entry[] }>('history', { botId, beforeSeq, limit })).entries
  }

  async botConversation(botId: string, peerId: string) {
    return (await this.call<{ entries: Entry[] }>('botConversation', { botId, peerId })).entries
  }

  /** `pasted`: what the user pasted rather than typed, which memory never takes as their own words. `replyTo`: the
   * chat message this one replies to, or the part of it the user selected (`replyPart`). */
  /** `call`: the voice call it was said in (on-device or speech to text). */
  async send(botId: string, text: string, clientNonce: string, attachments: string[] | null, pasted: string[] = [], replyTo: ReplyQuote | null = null, call: string | null = null) {
    const reply = replyTo ? { replyTo: replyTo.id, ...(replyTo.part ? { replyPart: replyTo.text } : {}) } : {}
    return (await this.call<{ entry: Entry }>('send', { botId, text, clientNonce, attachments, ...(pasted.length ? { pasted } : {}), ...reply, ...(call ? { call } : {}) })).entry
  }

  /** Holds a message for later (`scheduleSend`); the host's sweep delivers it when its time comes. */
  async scheduleSend(botId: string, text: string, notBefore: number, clientNonce: string, replyTo: ReplyQuote | null = null) {
    const reply = replyTo ? { replyTo: replyTo.id, ...(replyTo.part ? { replyPart: replyTo.text } : {}) } : {}
    return (await this.call<{ entry: Entry }>('scheduleSend', { botId, text, notBefore, clientNonce, ...reply })).entry
  }

  /** Edits a message that hasn't gone out: one held for later takes a new text and/or time; one waiting in
   * the live queue can be edited too, and a time holds it for then. The host answers with the entry when it
   * has one (a waiting message's follows in the store's events). */
  async updateScheduled(botId: string, entryId: string, patch: { text?: string; notBefore?: number; expectedRev?: number }) {
    const { expectedRev, ...rest } = patch
    return (await this.call<{ entry?: Entry }>('updateScheduled', {
      botId,
      entryId,
      ...rest,
      ...(expectedRev !== undefined ? { expectedRev } : {}),
    })).entry
  }

  /** Takes a scheduled message back before it fires: never delivered. */
  async cancelScheduled(botId: string, entryId: string) {
    await this.call('cancelScheduled', { botId, entryId })
  }

  /** Delivers a scheduled message right now: it enters the ordinary queue as the sweep would, with the text
   * the user has just edited, when given. `expectedRev` is the revision the sheet read: a newer save in
   * another window makes the host refuse this text instead of sending it over that one. */
  async sendScheduledNow(botId: string, entryId: string, expectedRev?: number, text?: string) {
    return (
      await this.call<{ entry: Entry }>('sendScheduledNow', {
        botId,
        entryId,
        ...(expectedRev !== undefined ? { expectedRev } : {}),
        ...(text !== undefined ? { text } : {}),
      })
    ).entry
  }

  /** Bytes per `upload` call: base64 of it stays well under the channel's 1 MiB message. */
  static uploadChunk = 384 * 1024

  /** One file for a message, in chunks: to `botId`'s chat, or with `taskId` to the thread of that task of the bot's;
   *  with no `botId` (''), for a thread starting without a bot. */
  async upload(botId: string, uploadId: string, name: string, data: Uint8Array, taskId?: string) {
    let offset = 0
    do {
      const end = Math.min(offset + HostClient.uploadChunk, data.length)
      await this.call('upload', { ...(botId ? { botId } : {}), ...(taskId ? { taskId } : {}), uploadId, name, data: toBase64(data.subarray(offset, end)), offset, done: end === data.length }, 60_000)
      offset = end
    } while (offset < data.length)
  }

  async readUpload(botId: string, uploadId: string) {
    const parts: Uint8Array[] = []
    let size = 0
    for (;;) {
      const res = await this.call<{ data: string; size: number }>('readUpload', { botId, uploadId, offset: size }, 60_000)
      const chunk = fromBase64(res.data)
      if (!chunk.length) break
      parts.push(chunk)
      size += chunk.length
      if (size >= res.size) break
    }
    const out = new Uint8Array(size)
    let at = 0
    for (const p of parts) {
      out.set(p, at)
      at += p.length
    }
    return out
  }
}

export function toBase64(bytes: Uint8Array) {
  let binary = ''
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000))
  return btoa(binary)
}

export function fromBase64(text: string) {
  const binary = atob(text)
  const out = new Uint8Array(binary.length)
  for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i)
  return out
}
