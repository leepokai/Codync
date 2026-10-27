package com.codync.android.core

import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.*
import java.io.IOException

class VoiceUnavailable(message: String) : IOException(message)
fun callDuration(seconds: Long): String {
    require(seconds >= 0)
    return (seconds / 60).toString().padStart(2, '0') + ":" + (seconds % 60).toString().padStart(2, '0')
}

/** Match the existing iOS spoken reply transformation, without sending audio to the host. */
object SpokenText {
    private val rules = listOf(
        "```[\\s\\S]*?```" to "",
        "!?\\[([^\\]]*)\\]\\([^)]*\\)" to "$1",
        "`([^`]*)`" to "$1",
        "(?m)^\\s{0,3}(#{1,6}|>|[-*+]|\\d+\\.)\\s+" to "",
        "(\\*\\*|\\*|~~)(\\S[^\\n]*?)\\1" to "$2",
        "(?m)^[ \\t]*\\|?[ \\t:|-]{3,}\\|?[ \\t]*$" to "",
        "(?m)^[ \\t]*\\|[ \\t]*|[ \\t]*\\|[ \\t]*$" to "",
        "[ \\t]*\\|[ \\t]*" to ", ", "\\n{2,}" to "\n",
    ).map { (pattern, replacement) -> Regex(pattern) to replacement }
    fun from(markdown: String): String {
        require(markdown.length <= 256 * 1024) { "This reply is too long to read aloud." }
        var text = markdown
        for ((pattern, replacement) in rules) text = pattern.replace(text, replacement)
        return text.trim()
    }
    /** Android TTS limits each utterance; split without cutting a UTF-16 surrogate pair. */
    fun chunks(text: String, limit: Int = 3_500): List<String> {
        require(limit >= 2)
        val result = mutableListOf<String>(); var offset = 0
        while (offset < text.length) {
            var end = minOf(offset + limit, text.length)
            if (end < text.length && text[end - 1].isHighSurrogate() && text[end].isLowSurrogate()) end--
            result.add(text.substring(offset, end)); offset = end
        }
        return result
    }
}

/**
 * A call's audio, the same on every engine (and on the iPhone and the desktop): connecting until the provider confirmed
 * the session and the microphone delivers, then listening or speaking; reconnecting after a drop; interrupted while
 * another app has the audio; failed when it can't go on; ended.
 */
enum class VoicePhase { Starting, Listening, Speaking, Reconnecting, Interrupted, Failed, Ended;
    /** The connecting line shows: not ready yet, or ready again soon. */
    val connecting: Boolean get() = this == Starting || this == Reconnecting
}
/** Why a call's engine gave up, for analytics (`voice_call_quality`). */
enum class VoiceFailureKind(val wire: String) { Microphone("microphone"), Connection("connection"), Provider("provider") }
data class VoiceSettings(val pauseMillis: Long = 1_500, val rate: Float = 1f) {
    init { require(pauseMillis in listOf(1_000L, 1_500L, 2_500L)); require(rate in .5f..2f) }
}
data class VoiceState(val phase: VoicePhase = VoicePhase.Starting, val muted: Boolean = false,
    val heard: String = "", val said: String = "", val level: Float = 0f, val engine: String = "",
    val settings: VoiceSettings = VoiceSettings(), val error: String? = null,
    val searchResult: CallSearchResult? = null,
    /** Where the audio goes when it leaves the phone; null on device. */
    val provider: VoiceProvider? = null,
    /** Why it failed, with `error`. */
    val failureKind: VoiceFailureKind? = null)
sealed interface VoiceInput {
    data class Text(val generation: Long, val value: String, val final: Boolean) : VoiceInput
    data class Level(val generation: Long, val value: Float) : VoiceInput
    data class Failed(val generation: Long, val message: String) : VoiceInput
}
interface VoiceAudio {
    suspend fun initialize(): String
    suspend fun listen(generation: Long, events: (VoiceInput) -> Unit)
    suspend fun stopListening()
    suspend fun speak(id: Long, text: String, rate: Float, completed: (Long, String?) -> Unit)
    suspend fun stopSpeaking()
    suspend fun close()
}

/** Serial audio ownership: no late recognizer/TTS callback can submit or restart a retired call. */
class VoiceSession(parent: CoroutineScope, private val audio: VoiceAudio, settings: VoiceSettings,
    private val send: (String) -> Unit) : VoiceEngine {
    private sealed interface Event {
        data class Input(val value: VoiceInput) : Event
        data class Submit(val generation: Long, val revision: Long) : Event
        data class Reply(val value: String) : Event
        data class Done(val id: Long, val error: String?) : Event
        data class Mute(val value: Boolean) : Event
        data class Settings(val value: VoiceSettings) : Event
        data class Focus(val active: Boolean) : Event
        data object Interrupt : Event
    }
    private val job = SupervisorJob(parent.coroutineContext[Job])
    private val scope = CoroutineScope(parent.coroutineContext + job)
    private val commands = Channel<Event>(64)
    private val mutableState = MutableStateFlow(VoiceState(settings = settings))
    override val state = mutableState.asStateFlow()
    private var generation = 0L; private var revision = 0L; private var utterance = 0L
    private var silence: Job? = null
    private val replies = ArrayDeque<String>()
    init { scope.launch {
        try {
            val engine = audio.initialize()
            mutableState.update { it.copy(engine = engine) }
            listen()
            for (event in commands) {
                try { receive(event) }
                catch (cancelled: CancellationException) { throw cancelled }
                catch (_: Exception) { fail("Couldn't use voice audio. End the call and check the microphone and installed voices.") }
            }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) { fail(if (error is VoiceUnavailable) error.message.orEmpty()
            else "Couldn't start voice audio. Check the microphone and installed voices.") }
        finally {
            commands.close(); silence?.cancel(); replies.clear()
            try { withContext(NonCancellable) { audio.close() } }
            catch (_: Exception) { mutableState.update { it.copy(error = it.error ?: "Voice audio couldn't finish closing. Start a new call when Android releases the microphone.") } }
            finally {
                mutableState.update { it.copy(phase = VoicePhase.Ended, heard = "", said = "", level = 0f) }
                job.cancel()
            }
        }
    } }
    private fun post(event: Event): Boolean = job.isActive && commands.trySend(event).isSuccess
    override fun speak(markdown: String): Boolean = post(Event.Reply(markdown))
    override fun announce(notice: String) { post(Event.Reply(notice)) }
    override fun mute(value: Boolean) { post(Event.Mute(value)) }
    override fun settings(value: VoiceSettings) { post(Event.Settings(value)) }
    override fun focus(active: Boolean) { post(Event.Focus(active)) }
    override fun interrupt() { post(Event.Interrupt) }
    override fun end() { job.cancel() }
    override suspend fun awaitEnd() { job.join() }

    private suspend fun stopListening() {
        generation++; revision++; silence?.cancel(); silence = null
        audio.stopListening()
        mutableState.update { it.copy(level = 0f) }
    }
    private suspend fun listen() {
        mutableState.update { it.copy(phase = VoicePhase.Listening, heard = "", level = 0f) }
        if (state.value.muted) return
        generation++
        audio.listen(generation) { post(Event.Input(it)) }
    }
    private suspend fun speakNext() {
        val next = replies.removeFirstOrNull()
        if (next == null) { listen(); return }
        utterance++
        mutableState.update { it.copy(phase = VoicePhase.Speaking, said = next, heard = "") }
        try { audio.speak(utterance, next, state.value.settings.rate) { id, error -> post(Event.Done(id, error)) } }
        catch (unavailable: VoiceUnavailable) {
            // A reply the phone has no voice for stays in chat; the call goes on.
            mutableState.update { it.copy(error = unavailable.message) }
            speakNext()
        }
    }
    private suspend fun submit() {
        val text = state.value.heard.trim()
        stopListening()
        if (text.isNotEmpty()) send(text)
        listen() // Bot work never stops listening to the next utterance.
    }
    private suspend fun fail(message: String) {
        stopListening(); utterance++; replies.clear(); audio.stopSpeaking()
        mutableState.update { it.copy(phase = VoicePhase.Failed, error = message) }
    }
    private suspend fun receive(event: Event) {
        if (state.value.phase == VoicePhase.Failed) return
        when (event) {
            is Event.Input -> when (val input = event.value) {
                is VoiceInput.Text -> if (input.generation == generation && state.value.phase == VoicePhase.Listening && !state.value.muted) {
                    require(input.value.length <= 16 * 1024)
                    if (input.value != state.value.heard) {
                        mutableState.update { it.copy(heard = input.value) }; revision++; silence?.cancel()
                        val owner = generation; val version = revision
                        if (input.value.isNotBlank()) silence = scope.launch {
                            delay(state.value.settings.pauseMillis); post(Event.Submit(owner, version))
                        }
                    }
                    if (input.final) submit()
                }
                is VoiceInput.Level -> if (input.generation == generation && state.value.phase == VoicePhase.Listening && !state.value.muted)
                    mutableState.update { it.copy(level = if (input.value.isFinite()) input.value.coerceIn(0f, 1f) else 0f) }
                is VoiceInput.Failed -> if (input.generation == generation) fail(input.message)
            }
            is Event.Submit -> if (event.generation == generation && event.revision == revision && state.value.phase == VoicePhase.Listening && !state.value.muted) submit()
            is Event.Reply -> {
                if (event.value.length > 256 * 1024) {
                    mutableState.update { it.copy(error = "This reply is too long to read aloud. Read it in chat.") }; return
                }
                val text = SpokenText.from(event.value)
                if (text.isNotBlank()) {
                    if (replies.size >= 8) { mutableState.update { it.copy(error = "More replies are waiting in chat than voice can read. Open the conversation.") }; return }
                    replies.addLast(text)
                    if (state.value.phase == VoicePhase.Listening) { stopListening(); speakNext() }
                }
            }
            is Event.Done -> if (event.id == utterance && state.value.phase == VoicePhase.Speaking) {
                if (event.error != null) fail(event.error) else speakNext()
            }
            is Event.Mute -> {
                mutableState.update { it.copy(muted = event.value) }
                if (state.value.phase == VoicePhase.Listening) { stopListening(); listen() }
            }
            is Event.Settings -> mutableState.update { it.copy(settings = event.value) }
            is Event.Focus -> if (!event.active) {
                stopListening(); utterance++; replies.clear(); audio.stopSpeaking()
                mutableState.update { it.copy(phase = VoicePhase.Interrupted, heard = "", said = "") }
            } else if (state.value.phase == VoicePhase.Interrupted) {
                if (replies.isEmpty()) listen() else speakNext()
            }
            Event.Interrupt -> if (state.value.phase == VoicePhase.Speaking) {
                utterance++; replies.clear(); audio.stopSpeaking(); listen()
            }
        }
    }
}
