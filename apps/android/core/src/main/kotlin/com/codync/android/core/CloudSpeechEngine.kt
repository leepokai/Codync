package com.codync.android.core

import java.io.IOException
import kotlinx.coroutines.*
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.update

/**
 * The provider's speech models without a live conversation (iOS `CloudSpeechEngine`): each utterance
 * is cut at a pause, transcribed with the provider's speech to text and sent as an ordinary message;
 * the bot's replies are read aloud with its text to speech. Both go through the computer, which holds the key.
 */
class CloudSpeechEngine(parent: CoroutineScope, provider: VoiceProvider, settings: VoiceSettings,
    private val mic: MicSource, private val player: ClipPlayer, private val speech: Speech,
    private val clock: () -> Long = { System.nanoTime() / 1_000_000 },
) : ProviderEngine(parent, provider, settings, "${provider.title} speech") {
    class Speech(val transcribe: suspend (ByteArray) -> String, val synthesize: suspend (String) -> Pair<ByteArray, String>,
        val send: (String) -> Unit)

    private val detector = UtteranceDetector()
    /** Transcriptions finish in the order they were spoken. */
    private val utterances = Channel<ByteArray>(Channel.UNLIMITED)
    /** The reply's clips still to fetch and play. */
    private val queue = ArrayDeque<String>()
    private var speaking: Job? = null
    /** Playback just ended; the room's echo of it isn't speech. */
    private var deafUntil = 0L

    init {
        hop {
            try {
                mic.start({ pcm, level -> hop { heard(pcm, level) } }) {
                    hop { fail("Couldn't use the microphone. Check its permission and other audio apps.", VoiceFailureKind.Microphone) }
                }
            } catch (error: IOException) {
                return@hop fail(error.message ?: "Couldn't use the microphone.", VoiceFailureKind.Microphone)
            }
            scope.launch {
                try {
                    for (wav in utterances) {
                        val text = speech.transcribe(wav).trim()
                        if (!ended && text.isNotEmpty()) speech.send(text)
                    }
                } catch (cancelled: CancellationException) { throw cancelled }
                catch (error: Exception) { hop { fail(error.message ?: "Couldn't transcribe what you said.", VoiceFailureKind.Provider) } }
            }
        }
    }

    override fun speak(markdown: String): Boolean {
        if (ended) return false
        hop { say(runCatching { SpokenText.from(markdown) }.getOrDefault("")) }
        return true
    }
    override fun announce(notice: String) = hop { say(notice) }
    override fun mute(value: Boolean) = hop {
        mutable.update { it.copy(muted = value) }
        if (value) { detector.reset(); level(0f) }
    }
    override fun interrupt() = hop { if (phase == VoicePhase.Speaking) { stopSpeaking(); phase(VoicePhase.Listening) } }
    override fun focus(active: Boolean) = hop {
        if (!active && this.active) { stopSpeaking(); detector.reset(); level(0f); phase(VoicePhase.Interrupted) }
        else if (active && phase == VoicePhase.Interrupted) { phase(VoicePhase.Listening); if (queue.isNotEmpty()) playQueue() }
    }

    private fun heard(pcm: ByteArray, level: Float) {
        // Listening once the microphone actually delivers.
        if (!ended && phase == VoicePhase.Starting) phase(VoicePhase.Listening)
        if (muted || phase != VoicePhase.Listening || clock() < deafUntil) return
        level(level)
        detector.feed(pcm, level, clock(), state.value.settings.pauseMillis)?.let { utterances.trySend(it) }
    }

    /** Queues text in clips the provider can turn into one channel message each. */
    private fun say(text: String) {
        if (ended || phase == VoicePhase.Starting || text.isEmpty()) return
        queue.addAll(speechClips(text, provider.speechChunk))
        if (phase != VoicePhase.Interrupted) playQueue()
    }

    /** Plays the queue, fetching each clip while the one before it plays. A clip that can't be made ends the reply. */
    private fun playQueue() {
        if (speaking != null) return
        speaking = scope.launch {
            coroutineScope {
                fun fetch(text: String) = async {
                    try { speech.synthesize(text) } catch (cancelled: CancellationException) { throw cancelled } catch (_: Exception) { null }
                }
                var next: Deferred<Pair<ByteArray, String>?>? = null
                while (true) {
                    val pending = next ?: queue.removeFirstOrNull()?.let(::fetch) ?: break
                    next = null
                    val clip = pending.await()
                    if (clip == null) { queue.clear(); break }
                    detector.reset(); level(0f); phase(VoicePhase.Speaking)
                    queue.removeFirstOrNull()?.let { next = fetch(it) }
                    try { player.play(clip.first, clip.second) }
                    catch (cancelled: CancellationException) { throw cancelled }
                    catch (_: Exception) { /* An unplayable clip is skipped. */ }
                }
            }
            speaking = null
            deafUntil = clock() + 400
            if (phase == VoicePhase.Speaking) phase(VoicePhase.Listening)
        }
    }

    private fun stopSpeaking() { speaking?.cancel(); speaking = null; queue.clear() }

    override suspend fun release() {
        stopSpeaking(); utterances.close()
        mic.stop()
    }
}
