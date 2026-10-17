package com.codync.android.core

import kotlinx.coroutines.*
import kotlinx.coroutines.test.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class VoiceTest {
    @Test fun voiceDurationShowsElapsedTimeAndRejectsInvalidHostValues() {
        assertEquals("00:16", callDuration(16)); assertEquals("61:05", callDuration(3_665))
        val entry = buildJsonObject { put("id", "call"); put("seq", 1); put("rev", 1); put("botId", "bot"); put("kind", "notice")
            putJsonObject("data") { put("text", "Voice chat"); put("callSeconds", -1) } }
        assertTrue(runCatching { Entry.decode(entry) }.isFailure)
    }
    private class Audio : VoiceAudio {
        var generation = 0L; var input: ((VoiceInput) -> Unit)? = null
        var closed = 0; var stops = 0; var speakerStops = 0
        var failedInit = false; var failedClose = false
        val spoken = mutableListOf<Pair<Long, String>>()
        var done: ((Long, String?) -> Unit)? = null
        /** Replies containing this have no installed voice. */
        var unvoiced: String? = null
        override suspend fun initialize(): String { if (failedInit) error("Fixture unavailable"); return "Fixture local engine" }
        override suspend fun listen(generation: Long, events: (VoiceInput) -> Unit) { this.generation = generation; input = events }
        override suspend fun stopListening() { stops++ }
        override suspend fun speak(id: Long, text: String, rate: Float, completed: (Long, String?) -> Unit) {
            if (unvoiced?.let(text::contains) == true) throw VoiceUnavailable("Install an offline Chinese voice")
            spoken.add(id to text); done = completed
        }
        override suspend fun stopSpeaking() { speakerStops++ }
        override suspend fun close() { closed++; if (failedClose) error("Fixture close failure") }
        fun text(value: String, final: Boolean = false, owner: Long = generation) { input?.invoke(VoiceInput.Text(owner, value, final)) }
    }
    @Test fun markdownBecomesSpeechAndChunksKeepSurrogatePairsIntact() {
        val text = SpokenText.from("# Title\n\n**Hello**, [friend](https://example.com) and `code`.\n```sh\nsecret command\n```\n| A | B |\n|---|---|\n| one | two |")
        assertEquals("Title\nHello, friend and code.\nA, B\none, two", text)
        val value = "abc🐈def🐈ghi"
        val chunks = SpokenText.chunks(value, 4)
        assertEquals(value, chunks.joinToString(""))
        assertTrue(chunks.all { it.length <= 4 && !it.last().isHighSurrogate() && !it.first().isLowSurrogate() })
    }
    @Test fun unchangedPartialResultsDoNotDelaySubmissionAndLateFinalsCannotResend() = runTest {
        val audio = Audio(); val sent = mutableListOf<String>()
        val session = VoiceSession(backgroundScope, audio, VoiceSettings(), sent::add)
        runCurrent(); val original = audio.generation
        audio.text("  first utterance  "); runCurrent(); advanceTimeBy(1_000)
        audio.text("  first utterance  "); runCurrent(); advanceTimeBy(500); runCurrent()
        assertEquals(listOf("first utterance"), sent)
        audio.text("first utterance", final = true, owner = original); runCurrent()
        assertEquals(listOf("first utterance"), sent)
        audio.text("second utterance", final = true); runCurrent()
        assertEquals(listOf("first utterance", "second utterance"), sent)
        assertEquals(VoicePhase.Listening, session.state.value.phase)
        session.end(); runCurrent(); session.awaitEnd(); assertEquals(1, audio.closed)
    }
    @Test fun muteAndAudioInterruptionDiscardPartialSpeechAndRespectMuteOnReturn() = runTest {
        val audio = Audio(); val sent = mutableListOf<String>()
        val session = VoiceSession(backgroundScope, audio, VoiceSettings(), sent::add)
        runCurrent(); val original = audio.generation
        audio.text("Never send this"); runCurrent(); session.mute(true); runCurrent()
        audio.text("Never send this", true, original); advanceTimeBy(3_000); runCurrent()
        assertTrue(sent.isEmpty()); assertTrue(session.state.value.muted)
        session.focus(false); runCurrent(); assertEquals(VoicePhase.Interrupted, session.state.value.phase)
        session.focus(true); runCurrent(); assertTrue(session.state.value.muted)
        session.mute(false); runCurrent(); audio.text("After unmute", true); runCurrent()
        assertEquals(listOf("After unmute"), sent)
        session.end(); runCurrent(); session.awaitEnd()
    }
    @Test fun aReplyWithNoInstalledVoiceIsSkippedWithANoticeAndTheCallGoesOn() = runTest {
        val audio = Audio().apply { unvoiced = "測試" }
        val session = VoiceSession(backgroundScope, audio, VoiceSettings()) { }
        runCurrent()
        session.speak("測試通過了"); session.speak("Second reply"); runCurrent()
        assertEquals("Install an offline Chinese voice", session.state.value.error)
        assertEquals(listOf("Second reply"), audio.spoken.map { it.second })
        assertEquals(VoicePhase.Speaking, session.state.value.phase)
        audio.done?.invoke(audio.spoken.single().first, null); runCurrent()
        assertEquals(VoicePhase.Listening, session.state.value.phase)
        session.end(); runCurrent(); session.awaitEnd()
    }
    @Test fun repliesPauseTheMicrophoneAndStalePlaybackCompletionCannotRestartIt() = runTest {
        val audio = Audio(); val session = VoiceSession(backgroundScope, audio, VoiceSettings()) { fail("Playback audio must not submit text") }
        runCurrent(); val recognition = audio.generation
        session.speak("**First** reply"); runCurrent()
        assertEquals(VoicePhase.Speaking, session.state.value.phase)
        audio.text("Playback echo", true, recognition); runCurrent()
        session.speak("Second reply"); runCurrent()
        val first = audio.spoken.single().first
        audio.done?.invoke(first, null); runCurrent()
        assertEquals(listOf("First reply", "Second reply"), audio.spoken.map { it.second })
        session.interrupt(); runCurrent(); val resumed = audio.generation
        audio.done?.invoke(audio.spoken.last().first, null); runCurrent()
        assertEquals(resumed, audio.generation)
        session.end(); runCurrent(); session.awaitEnd()
        audio.done?.invoke(first, null); runCurrent(); assertEquals(VoicePhase.Ended, session.state.value.phase)
    }
    @Test fun initializationFailureAndParentCancellationReleaseAudioExactlyOnce() = runTest {
        val unavailable = Audio().apply { failedInit = true }
        val failed = VoiceSession(backgroundScope, unavailable, VoiceSettings()) { fail("Failed call cannot send") }
        runCurrent(); failed.awaitEnd(); assertNotNull(failed.state.value.error); assertEquals(1, unavailable.closed)
        val owner = Job(backgroundScope.coroutineContext[Job]); val parent = CoroutineScope(backgroundScope.coroutineContext + owner)
        val audio = Audio(); val session = VoiceSession(parent, audio, VoiceSettings()) { fail("Retired call cannot send") }
        runCurrent(); owner.cancel(); runCurrent(); session.awaitEnd()
        audio.text("Late callback", true); runCurrent(); assertEquals(1, audio.closed)
    }
    @Test fun aFailedAudioCloseStillEndsTheCallAndRejectsLateRecognition() = runTest {
        val audio = Audio().apply { failedClose = true }
        val session = VoiceSession(backgroundScope, audio, VoiceSettings()) { fail("Ended call cannot send") }
        runCurrent(); session.end(); runCurrent(); session.awaitEnd()
        assertEquals(VoicePhase.Ended, session.state.value.phase)
        assertNotNull(session.state.value.error); assertEquals(1, audio.closed)
        audio.text("Late recognition", true); runCurrent()
        assertFalse(session.speak("A late reply")); assertEquals(1, audio.closed)
    }
    @Test fun replyGateIgnoresHistoryOtherBotsThreadsAndAlreadyFinalizedReplies() {
        fun entry(id: String, rev: Long, bot: String = "active", thread: String? = null, final: Boolean = true) = Entry(
            id, rev, bot, thread, rev, "agent", data = buildJsonObject { put("text", id); put("final", final) })
        val old = entry("old", 5); val streaming = entry("streaming", 9, final = false)
        val gate = VoiceReplyGate("active", 10, listOf(old, streaming))
        val values = listOf(entry("old", 20), entry("streaming", 21), entry("reply", 22),
            entry("thread", 23, thread = "root"), entry("other", 24, bot = "other"), entry("history", 6))
        val bots = listOf(Bot("active", "Ada"))
        assertEquals(listOf("streaming", "reply"), gate.observe(values, bots).map(VoiceCue::text))
        assertTrue(gate.observe(values, bots).isEmpty())
        val permission = Entry("approval", 25, "active", rev = 25, kind = "permission", data = buildJsonObject { put("status", "pending") })
        assertEquals(listOf(VoiceCue("Ada needs your approval in the chat.", notice = true)), gate.observe(values + permission, bots))
        assertTrue(gate.observe(values + permission, bots).isEmpty())
    }
    @Test fun groupRepliesAndApprovalsNameTheMemberWhoSentThem() {
        val bots = listOf(Bot("team", "Team", kind = "group", members = listOf("ada")), Bot("ada", "Ada"))
        val gate = VoiceReplyGate("team", 0, emptyList())
        val reply = Entry("r", 1, "team", rev = 1, kind = "agent", data = buildJsonObject { put("text", "Done"); put("final", true); put("author", "ada") })
        val card = Entry("p", 2, "team", rev = 2, kind = "permission", data = buildJsonObject { put("status", "pending"); put("author", "ghost") })
        assertEquals(listOf(VoiceCue("Ada: Done"), VoiceCue("A bot needs your approval in the chat.", true)), gate.observe(listOf(reply, card), bots))
    }
}
