import type { AnalyticsEvent, VoiceCallEngine, VoiceFailure, VoiceStuck } from '@shared/analytics'
import type { VoiceFailureKind, VoicePhase } from './voice'

const isConnecting = (phase: VoicePhase) => phase === 'starting' || phase === 'reconnecting'

/**
 * How each engine of a call fares, for analytics (never content; kit `CallQuality`): one `voice_call_quality` per
 * engine, sent when it's replaced or the call ends — milliseconds until it was ready, reconnects, the first phase
 * stuck too long, why it failed.
 */
export class CallQuality {
  /** Connecting (or reconnecting) this long (ms) counts as stuck. */
  static readonly stuckMs = 20_000
  private engine: VoiceCallEngine | null = null
  private startedAt = 0
  private readyMs: number | undefined
  private reconnects = 0
  private stuck: VoiceStuck = 'none'
  private failure: VoiceFailure = 'none'
  private last: VoicePhase = 'starting'
  /** When the current connecting phase began. */
  private since = 0

  private capture: (event: AnalyticsEvent) => void
  private now: () => number

  constructor(capture: (event: AnalyticsEvent) => void, now: () => number = Date.now) {
    this.capture = capture
    this.now = now
  }

  /** A new engine takes the call; the previous one's report goes out. */
  begin(engine: VoiceCallEngine) {
    this.report()
    this.engine = engine
    this.startedAt = this.now()
    this.since = this.startedAt
    this.readyMs = undefined
    this.reconnects = 0
    this.stuck = 'none'
    this.failure = 'none'
    this.last = 'starting'
  }

  phase(phase: VoicePhase, failure: VoiceFailureKind | null) {
    if (!this.engine || phase === this.last) return
    if (isConnecting(this.last)) this.checkStuck()
    if (phase === 'listening' || phase === 'speaking') {
      this.readyMs ??= this.now() - this.startedAt
      if (this.last === 'reconnecting') this.reconnects++
    } else if (isConnecting(phase)) {
      this.since = this.now()
    } else if (phase === 'failed') {
      this.failure = failure ?? 'connection'
    }
    this.last = phase
  }

  /** Playback stopped reporting progress after the voice's turn (the engine's watchdog took the call back). */
  stalled() {
    if (this.stuck === 'none') this.stuck = 'speaking'
  }

  /** The engine is done (replaced, or the call ended): its report. */
  report() {
    if (!this.engine) return
    if (isConnecting(this.last)) this.checkStuck()
    this.capture({
      event: 'voice_call_quality',
      engine: this.engine,
      ...(this.readyMs === undefined ? {} : { ready_ms: this.readyMs }),
      reconnects: this.reconnects,
      stuck: this.stuck,
      failure: this.failure,
    })
    this.engine = null
  }

  private checkStuck() {
    if (this.stuck !== 'none' || this.now() - this.since < CallQuality.stuckMs) return
    this.stuck = this.last === 'starting' ? 'connecting' : 'reconnecting'
  }
}
