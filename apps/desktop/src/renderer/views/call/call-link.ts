import type { CallLine } from '@shared/calls'
import { sleep } from '../../store/bot-sync'
import type { BotStore } from '../../store/bot-store'
import { PROGRESS_TAG, spokenText, type VoiceProvider } from './voice'
import { requireLiveHost } from './openai-live-host'

/** How long a tool call waits for the user's words that are still being written down. */
const HEARING_PATIENCE_MS = 2_000

/** What a client needs to run a call's voice, from the computer (`startCall`). */
export interface CallSession {
  /** A realtime provider's short-lived credential. */
  credential?: string
  /** The voice's instructions: its role, then the background. */
  instructions: string
  /** The voice's tools in JSON Schema form. */
  tools: { name: string; description: string; parameters?: Record<string, unknown> }[]
}

/**
 * A call as the computer runs it (host/src/chat/calls, kit `CallLink`): the voice's instructions, tools and
 * transcript live there; the engines only carry audio. One per call, so it outlives an engine switch.
 */
export class CallLink {
  private lines: CallLine[] = []
  private open = false
  /** The user's last words are still being written down (OpenAI transcribes them after the turn). */
  private hearing = false
  /** The computer gets what was said, the tool calls and the end one at a time, in order: a hand-off never
   * overtakes the words it hands on. */
  private queue: Promise<unknown> = Promise.resolve()

  constructor(
    private store: BotStore,
    readonly botId: string,
    /** The call's id: what it sends the bot carries it. */
    readonly call: string,
  ) {}

  /** Starts the call on the computer (again, when a provider session is replaced): what the voice needs. */
  async start(mode: 'realtime' | 'speech' | 'device', provider: VoiceProvider | null = null, model?: string, voice?: string) {
    const client = await this.store.ready()
    return client.call<CallSession>('startCall', {
      botId: this.botId, callId: this.call, mode, provider, model, voice, language: navigator.language,
    }, 30_000)
  }

  async startLive(model: string, voice: string, sdp: string) {
    const client = await this.store.ready()
    await requireLiveHost(client)
    return client.call<{ sessionId: string; sdp: string }>('startLiveCall', {
      botId: this.botId, callId: this.call, model, voice, sdp, language: navigator.language,
    }, 30_000)
  }

  async liveEvent(sessionId: string, event: unknown) {
    return (await this.store.ready()).call<{ events: unknown[] }>('callLiveEvent', {
      callId: this.call, sessionId, event,
    }, 60_000)
  }

  async liveUpdate(sessionId: string, kind: 'reply' | 'notice' | 'progress', text: string) {
    return (await this.store.ready()).call<{ events: unknown[] }>('callLiveUpdate', {
      callId: this.call, sessionId, kind, text,
    }, 30_000)
  }

  /** One of the voice's tool calls, run on the computer with what was said that it hasn't got yet. A failure is the
   * answer. */
  async tool(name: string, args: unknown): Promise<unknown> {
    for (let waited = 0; this.hearing && waited < HEARING_PATIENCE_MS; waited += 100) await sleep(100)
    return this.serial(async () => {
      const lines = this.drain(true)
      try {
        const client = await this.store.ready()
        return (await client.call<{ result: unknown }>('callTool', { callId: this.call, name, arguments: args ?? {}, lines }, 60_000)).result
      } catch (e) {
        this.restore(lines)
        return { error: e instanceof Error ? e.message : String(e) }
      }
    })
  }

  /** A piece of what `from` said; the pieces of one turn join into one line. */
  add(from: CallLine['from'], piece: string) {
    if (from === 'user') this.hearing = false
    if (!piece) return
    const last = this.lines.at(-1)
    if (this.open && last?.from === from) {
      last.text += piece
    } else {
      this.endTurn()
      this.lines.push({ from, text: piece, at: Date.now(), id: crypto.randomUUID() })
      this.open = true
    }
  }

  /** The user stopped talking and their words are being written down (`true`), or that failed (`false`): a tool
   * call waits a moment for them. */
  transcribing(on: boolean) {
    this.hearing = on
  }

  /** The turn is over: what was said goes to the computer, and the next piece starts a line. */
  endTurn() {
    this.open = false
    void this.serial(async () => {
      const lines = this.drain(false)
      if (!lines.length) return
      try {
        await (await this.store.ready()).call('callLines', { callId: this.call, lines })
      } catch {
        this.restore(lines)
      }
    })
  }

  /** How a final reply of the bot reaches the voice. */
  reply(text: string) {
    return `[${this.store.bots.get(this.botId)?.name ?? 'Bot'} replied] ${spokenText(text)}`
  }

  notice(text: string) {
    return `[Notice] ${text}`
  }

  /** What the bot is doing, as the voice's instructions name it. */
  progress(text: string) {
    return `${PROGRESS_TAG} ${text}`
  }

  /** The call is over: its line in the chat, with what's left of the transcript. */
  end(seconds: number) {
    void this.serial(() => this.store.endCall(this.botId, this.call, seconds, this.drain(true)))
  }

  /** The lines finished so far (`withOpen`: and the one still being said), trimmed and out of the buffer. */
  private drain(withOpen: boolean): CallLine[] {
    const count = this.open && !withOpen ? this.lines.length - 1 : this.lines.length
    if (withOpen) this.open = false
    // ponytail: kit turns Simplified Chinese into Traditional here (ICU Hans-Hant); Chromium has no such transform.
    return this.lines.splice(0, count).map((l) => ({ ...l, text: l.text.trim() })).filter((l) => l.text)
  }

  /** Lines the computer didn't get go out with the next request. */
  private restore(lines: CallLine[]) {
    this.lines.unshift(...lines)
  }

  private serial<T>(op: () => Promise<T>): Promise<T> {
    const run = this.queue.then(op)
    this.queue = run.catch(() => {})
    return run
  }
}
