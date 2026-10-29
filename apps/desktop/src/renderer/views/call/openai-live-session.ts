import type { CallLink } from './call-link'

export const isLiveModel = (model: string) => model.startsWith('gpt-live-') && !model.includes('transcribe')
export type LiveUpdateKind = 'reply' | 'notice' | 'progress'

/** The host owns Live's transcripts and Responses tool routing. This adapter only carries its events. */
export class OpenAILiveSession {
  private sessionId: string | null = null
  private stopped = false
  private closing = false
  private queue: Promise<void> = Promise.resolve()
  private closed: (() => void) | null = null
  private queued = 0
  private link: Pick<CallLink, 'startLive' | 'liveEvent' | 'liveUpdate'>
  private send: (event: unknown) => void
  private failed: (message: string) => void

  constructor(
    link: Pick<CallLink, 'startLive' | 'liveEvent' | 'liveUpdate'>,
    send: (event: unknown) => void,
    failed: (message: string) => void,
  ) { this.link = link; this.send = send; this.failed = failed }

  async start(model: string, voice: string, sdp: string): Promise<string> {
    const answer = await this.link.startLive(model, voice, sdp)
    if (this.stopped) throw new Error('The voice connection was replaced.')
    this.sessionId = answer.sessionId
    return answer.sdp
  }

  receive(event: Record<string, unknown>) {
    if (event.type === 'session.closed') {
      this.closed?.()
      if (!this.closing && !this.stopped) this.failed('OpenAI closed the Live session.')
      return
    }
    if (this.stopped || this.closing) return
    if (event.type === 'error' || event.type === 'session.error') {
      this.failed((event.error as { message?: string } | undefined)?.message ?? 'OpenAI Live reported an error.')
      return
    }
    if (!['response.event', 'session.input_transcript.delta', 'session.output_transcript.delta', 'session.delegation.created'].includes(String(event.type))) return
    this.enqueue((id) => this.link.liveEvent(id, event))
  }

  update(kind: LiveUpdateKind, text: string) {
    this.enqueue((id) => this.link.liveUpdate(id, kind, text))
  }

  /** Invalidate every outstanding host result before a replacement connection is created. */
  discard() { this.stopped = true }

  async shutdown() {
    if (this.closing) return
    this.closing = true
    let timer: ReturnType<typeof setTimeout> | undefined
    const confirmed = await new Promise<boolean>((resolve) => {
      this.closed = () => resolve(true)
      timer = setTimeout(() => resolve(false), 1_000)
      this.send({ type: 'session.close' })
    })
    clearTimeout(timer)
    this.stopped = true
    this.closed = null
    if (!confirmed) console.warn('OpenAI Live closed without a final session.closed acknowledgement.')
  }

  private enqueue(run: (id: string) => Promise<{ events: unknown[] }>) {
    if (this.stopped || this.closing) return
    if (this.queued >= 256) {
      this.stopped = true
      this.failed('The computer cannot keep up with GPT-Live events.')
      return
    }
    this.queued++
    this.queue = this.queue.then(async () => {
      if (this.stopped || this.closing || !this.sessionId) return
      const result = await run(this.sessionId)
      if (this.stopped || this.closing) return
      for (const event of result.events) this.send(event)
    }).catch((error: unknown) => {
      if (!this.stopped && !this.closing) {
        this.stopped = true
        this.failed(error instanceof Error ? error.message : String(error))
      }
    }).finally(() => { this.queued-- })
  }
}

/** Live uses a complete SDP offer; stop waiting if the connection closes or ICE cannot gather. */
export async function gatheredOffer(pc: RTCPeerConnection): Promise<string> {
  if (pc.iceGatheringState !== 'complete') {
    await new Promise<void>((resolve, reject) => {
      const finish = (error?: Error) => {
        clearTimeout(timer)
        pc.removeEventListener('icegatheringstatechange', changed)
        pc.removeEventListener('connectionstatechange', changed)
        if (error) reject(error); else resolve()
      }
      const changed = () => {
        if (pc.connectionState === 'closed') finish(new Error('The voice connection closed.'))
        else if (pc.iceGatheringState === 'complete') finish()
      }
      const timer = setTimeout(() => finish(new Error('Could not gather the voice connection addresses.')), 10_000)
      pc.addEventListener('icegatheringstatechange', changed)
      pc.addEventListener('connectionstatechange', changed)
      changed()
    })
  }
  const sdp = pc.localDescription?.sdp
  if (!sdp) throw new Error('The voice connection produced no SDP offer.')
  return sdp
}
