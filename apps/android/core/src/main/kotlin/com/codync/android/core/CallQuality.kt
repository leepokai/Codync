package com.codync.android.core

/** The first phase of a call's engine that lasted too long, for analytics (`voice_call_quality`). */
enum class VoiceStuck(val wire: String) { None("none"), Connecting("connecting"), Reconnecting("reconnecting"), Speaking("speaking") }

/**
 * How each engine of a call fares, for analytics (never content; iOS `CallQuality`): one report per engine (`E`, how
 * the app names it), made when it's replaced or the call ends — milliseconds until it was ready, reconnects, the first
 * phase stuck too long, why it failed.
 */
class CallQuality<E : Any>(private val clock: () -> Long = System::currentTimeMillis, private val capture: (E, Report) -> Unit) {
    /** `failure` is null when the engine didn't fail. */
    data class Report(val readyMs: Long?, val reconnects: Int, val stuck: VoiceStuck, val failure: VoiceFailureKind?)

    private var engine: E? = null
    private var startedAt = 0L
    private var readyMs: Long? = null
    private var reconnects = 0
    private var stuck = VoiceStuck.None
    private var failure: VoiceFailureKind? = null
    private var last = VoicePhase.Starting
    /** When the current connecting phase began. */
    private var since = 0L

    /** A new engine takes the call; the previous one's report goes out. */
    fun begin(engine: E) {
        report()
        this.engine = engine
        startedAt = clock(); since = startedAt
        readyMs = null; reconnects = 0; stuck = VoiceStuck.None; failure = null; last = VoicePhase.Starting
    }

    fun phase(phase: VoicePhase, kind: VoiceFailureKind?) {
        if (engine == null || phase == last) return
        if (last.connecting) checkStuck()
        when {
            phase == VoicePhase.Listening || phase == VoicePhase.Speaking -> {
                if (readyMs == null) readyMs = clock() - startedAt
                if (last == VoicePhase.Reconnecting) reconnects++
            }
            phase.connecting -> since = clock()
            phase == VoicePhase.Failed -> failure = kind ?: VoiceFailureKind.Connection
            else -> { }
        }
        last = phase
    }

    /** Playback stopped reporting progress after the voice's turn (the engine's watchdog took the call back). */
    fun stalled() { if (stuck == VoiceStuck.None) stuck = VoiceStuck.Speaking }

    /** The engine is done (replaced, or the call ended): its report. */
    fun report() {
        val engine = engine ?: return
        if (last.connecting) checkStuck()
        capture(engine, Report(readyMs, reconnects, stuck, failure))
        this.engine = null
    }

    private fun checkStuck() {
        if (stuck == VoiceStuck.None && clock() - since >= STUCK_MILLIS) {
            stuck = if (last == VoicePhase.Starting) VoiceStuck.Connecting else VoiceStuck.Reconnecting
        }
    }

    companion object {
        /** Connecting (or reconnecting) this long counts as stuck. */
        const val STUCK_MILLIS = 20_000L
    }
}
