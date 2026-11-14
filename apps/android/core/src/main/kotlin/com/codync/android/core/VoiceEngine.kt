package com.codync.android.core

import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.*

/**
 * What carries a call's audio (iOS `VoiceEngine`): on-device speech (`VoiceSession`), or a provider
 * on the user's own key (`CloudSpeechEngine`, `GeminiLiveEngine`, `OpenAIRealtimeEngine`).
 * The call bar and the voice service only talk to this.
 */
interface VoiceEngine {
    val state: StateFlow<VoiceState>
    /** A new final reply from the bot to say; false once the call has ended. */
    fun speak(markdown: String): Boolean
    /** A notice from the computer (an approval is waiting) to say. */
    fun announce(notice: String)
    /** What the bot is doing while it works, from the computer: for a realtime voice to mention (engines that read
     * replies aloud have nothing to say about it). */
    fun progress(text: String) {}
    /** What the computer tells the call's voice, handed to the method that says it. */
    fun hear(cue: CallCue) {
        when (cue.kind) {
            CallCue.Kind.Reply -> speak(cue.text)
            CallCue.Kind.Notice -> announce(cue.text)
            CallCue.Kind.Progress -> progress(cue.text)
        }
    }
    fun mute(value: Boolean)
    fun settings(value: VoiceSettings)
    /** Android audio focus: false pauses the call's audio, true resumes it. */
    fun focus(active: Boolean)
    /** Cuts what's being said short. */
    fun interrupt()
    fun end()
    suspend fun awaitEnd()
}

/** How often a dropped provider connection is tried again before the call falls back to on-device voice: after these
 * waits (ms). Tests shorten them. */
object Reconnect { @Volatile var delays = listOf(1_000L, 2_000L, 4_000L) }

/** The microphone as 16 kHz mono PCM16 frames with their loudness (0…1), delivered on an audio thread. */
interface MicSource {
    /** Throws when the microphone can't open; `failed` reports it stopping later. */
    suspend fun start(frames: (ByteArray, Float) -> Unit, failed: () -> Unit)
    suspend fun stop()
}

/** Streams 24 kHz mono PCM16. `played` runs (on any thread) once a chunk has been heard; `stop` drops what's queued. */
interface PcmPlayer {
    fun play(pcm: ByteArray, played: () -> Unit)
    fun stop()
    suspend fun close()
}

/** Plays one clip (`aac` or `wav`) to its end; cancelling stops it. */
fun interface ClipPlayer { suspend fun play(audio: ByteArray, format: String) }

/**
 * The lifecycle shared by the provider engines: one state, and one inbox that runs every callback
 * from the audio and network threads in order on the caller's dispatcher, so engine fields are only
 * touched there. Ports are released once, on `end()` or a failure.
 */
abstract class ProviderEngine(parent: CoroutineScope, val provider: VoiceProvider, settings: VoiceSettings, label: String) : VoiceEngine {
    private val job = SupervisorJob(parent.coroutineContext[Job])
    protected val scope = CoroutineScope(parent.coroutineContext + job)
    protected val mutable = MutableStateFlow(VoiceState(engine = label, settings = settings, provider = provider))
    final override val state = mutable.asStateFlow()
    private val inbox = Channel<suspend () -> Unit>(Channel.UNLIMITED)
    private var teardown: Job? = null
    protected var ended = false; private set
    protected val phase: VoicePhase get() = state.value.phase
    protected val muted: Boolean get() = state.value.muted
    protected val active: Boolean get() = phase == VoicePhase.Listening || phase == VoicePhase.Speaking

    init {
        scope.launch {
            for (task in inbox) {
                if (ended) continue
                try { task() }
                catch (cancelled: CancellationException) { throw cancelled }
                catch (error: Exception) { fail(error.message ?: "Couldn't reach ${provider.title}.", VoiceFailureKind.Provider) }
            }
        }
    }

    /** Runs `block` in the inbox: from a port's thread, or from the app. */
    protected fun hop(block: suspend () -> Unit) { if (!ended) inbox.trySend(block) }
    protected fun phase(value: VoicePhase) = mutable.update { it.copy(phase = value) }
    protected fun level(value: Float) = mutable.update { it.copy(level = value) }
    /** Stops the audio and the connection; may run twice. */
    protected abstract suspend fun release()

    protected fun fail(message: String, kind: VoiceFailureKind = VoiceFailureKind.Connection) { if (!ended) finish(VoicePhase.Failed, message, kind) }
    override fun end() { if (!ended) finish(VoicePhase.Ended, null) }
    override suspend fun awaitEnd() { teardown?.join() ?: job.join() }
    override fun settings(value: VoiceSettings) = mutable.update { it.copy(settings = value) }

    private fun finish(final: VoicePhase, error: String?, kind: VoiceFailureKind? = null) {
        ended = true
        if (error != null) mutable.update { it.copy(phase = VoicePhase.Failed, error = error, failureKind = kind, level = 0f) }
        // Outside `job`, so cancelling the engine's own work can't cut the release short.
        teardown = scope.launch(NonCancellable) {
            try { release() }
            catch (_: Exception) { mutable.update { it.copy(error = it.error ?: "Voice audio couldn't finish closing. Start a new call when Android releases the microphone.") } }
            finally {
                if (final == VoicePhase.Ended) mutable.update { it.copy(phase = VoicePhase.Ended, level = 0f) }
                inbox.close(); job.cancel()
            }
        }
    }
}
