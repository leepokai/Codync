package com.codync.android.core

import java.io.IOException
import java.time.YearMonth
import java.util.Base64
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.*
import kotlinx.serialization.json.*
import okhttp3.OkHttpClient
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.Assert.*
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class VoiceEnginesTest {
    private class Prefs { val values = mutableMapOf<String, String>(); val choices = VoiceChoices({ values[it] }) { k, v -> if (v == null) values.remove(k) else values[k] = v } }

    @Test fun choicesKeepOnlyWhatTheUserPickedAndFollowNewerDefaults() {
        val prefs = Prefs(); val choices = prefs.choices
        assertNull(choices.provider)
        choices.provider = VoiceProvider.Gemini; assertEquals(VoiceProvider.Gemini, choices.provider)
        choices.choose(VoiceProvider.Gemini, VoiceChoices.Model.Realtime, VoiceProvider.Gemini.model)
        assertTrue(prefs.values.keys.none { it.startsWith("callModel.") })
        choices.remember(VoiceStatus(listOf(VoiceStatus.ProviderStatus("gemini", true, VoiceDefaults("live-9", "flash-9", "tts-9")),
            VoiceStatus.ProviderStatus("future", true, VoiceDefaults("x", "y", "z")))))
        assertEquals("live-9", choices.model(VoiceProvider.Gemini, VoiceChoices.Model.Realtime))
        assertEquals("tts-9", choices.model(VoiceProvider.Gemini, VoiceChoices.Model.Speech))
        choices.choose(VoiceProvider.Gemini, VoiceChoices.Model.Transcribe, "flash-8")
        assertEquals("flash-8", choices.model(VoiceProvider.Gemini, VoiceChoices.Model.Transcribe))
        prefs.values["callVoice.openai"] = "retired"; assertEquals("marin", choices.voice(VoiceProvider.OpenAI))
        prefs.values["callDefaults.openai"] = "{broken"; assertEquals(VoiceProvider.OpenAI.defaults, choices.defaults(VoiceProvider.OpenAI))
        assertFalse(choices.speechMode(VoiceProvider.OpenAI)); choices.speechMode(VoiceProvider.OpenAI, true); assertTrue(choices.speechMode(VoiceProvider.OpenAI))
        val october = YearMonth.of(2026, 10)
        choices.addUsage(VoiceProvider.OpenAI, 61, october); choices.addUsage(VoiceProvider.OpenAI, -5, october)
        assertEquals(2, choices.minutes(VoiceProvider.OpenAI, october)); assertEquals(0, choices.minutes(VoiceProvider.OpenAI, october.plusMonths(1)))
        assertEquals("61", prefs.values["callUsage.openai.2026-10"])
    }

    @Test fun aCallTakesAnotherEngineOnlyWhenWhatItsEngineIsBuiltFromChanges() {
        val choices = Prefs().choices; val gemini = VoiceProvider.Gemini
        assertEquals(choices.engineChoice(null, ""), choices.engineChoice(null, ""))
        assertNotEquals(choices.engineChoice(null, ""), choices.engineChoice(null, "ja-JP"))
        assertNotEquals(choices.engineChoice(gemini, ""), choices.engineChoice(VoiceProvider.OpenAI, ""))
        val seen = mutableListOf(choices.engineChoice(gemini, ""))
        for (change in listOf<() -> Unit>(
            { choices.choose(gemini, VoiceChoices.Model.Realtime, "live-9") },
            { choices.voice(gemini, "Puck") },
            { choices.speechMode(gemini, true) },
            { choices.choose(gemini, VoiceChoices.Model.Transcribe, "flash-9") },
            { choices.choose(gemini, VoiceChoices.Model.Speech, "tts-9") })) {
            change()
            val choice = choices.engineChoice(gemini, "")
            assertEquals(choice, choices.engineChoice(gemini, ""))
            assertFalse(choice, choice in seen)
            seen += choice
        }
    }

    @Test fun googleSearchOnlyChangesGeminiRealtimeEngineChoice() {
        val prefs = Prefs(); val choices = prefs.choices
        assertFalse(choices.googleSearch)
        assertFalse(WireJson.decodeFromString<VoiceStatus>("{\"providers\":[]}").googleSearch)
        assertTrue(WireJson.decodeFromString<VoiceStatus>("{\"providers\":[],\"googleSearch\":true}").googleSearch)
        val gemini = choices.engineChoice(VoiceProvider.Gemini, "")
        val openai = choices.engineChoice(VoiceProvider.OpenAI, "")
        choices.googleSearch = true
        assertTrue(VoiceChoices({ prefs.values[it] }) { _, _ -> }.googleSearch)
        assertNotEquals(gemini, choices.engineChoice(VoiceProvider.Gemini, ""))
        assertEquals(openai, choices.engineChoice(VoiceProvider.OpenAI, ""))
        choices.speechMode(VoiceProvider.Gemini, true)
        val speech = choices.engineChoice(VoiceProvider.Gemini, "")
        choices.googleSearch = false
        assertEquals(speech, choices.engineChoice(VoiceProvider.Gemini, ""))
    }

    @Test fun utterancesStartAtSpeechEndAtThePauseAndDropCoughs() {
        val detector = UtteranceDetector(); val frame = ByteArray(3_200)
        assertNull(detector.feed(frame, 0f, 0, 1_000))
        assertNull(detector.feed(frame, .8f, 100, 1_000))
        assertNull(detector.feed(frame, 0f, 200, 1_000)) // A cough: one loud frame, then quiet.
        assertNull(detector.feed(frame, 0f, 1_300, 1_000))
        for (t in 0 until 6) assertNull(detector.feed(frame, .8f, 2_000L + t * 100, 1_000))
        assertNull(detector.feed(frame, 0f, 2_700, 1_000))
        val wav = requireNotNull(detector.feed(frame, 0f, 3_700, 1_000))
        assertEquals("RIFF", String(wav, 0, 4)); assertEquals("WAVE", String(wav, 8, 4))
        assertEquals(44 + 8 * 3_200, wav.size) // The quiet frame before speech is kept as preroll.
        assertNull(detector.feed(frame, 0f, 5_000, 1_000))
        val long = ByteArray(UtteranceDetector.MAX_BYTES)
        assertNotNull(detector.feed(long, .9f, 6_000, 1_000)) // A monologue is cut at one message's size.
    }

    @Test fun clipsGroupSentencesAndCutLongOnes() {
        assertEquals(listOf("One. Two.", "Three!"), speechClips("One. Two. Three!", 10))
        assertEquals(listOf("abcde", "fghij", "k."), speechClips("abcdefghijk.", 5))
        assertTrue(speechClips("  ", 5).isEmpty())
        assertEquals(listOf(1, 0, 1, 0, 128, 62, 0, 0, 0, 125, 0, 0), Pcm.wav(ByteArray(0)).copyOfRange(20, 32).map { it.toInt() and 0xFF })
        assertEquals(1f, Pcm.level(ShortArray(4) { Short.MAX_VALUE }, 4), .01f); assertEquals(0f, Pcm.level(ShortArray(4), 4))
    }

    /** The computer's side of a call, for the engines' tests: one session, tools answered here. */
    private class Link(val credential: String = "ek_secret", val googleSearch: Boolean? = null) : CallLink {
        val tools = mutableListOf<Pair<String, JsonObject>>(); val lines = mutableListOf<Pair<String, String>>(); var turns = 0
        var started: Triple<VoiceProvider, String, String>? = null; var starts = 0
        override suspend fun start(provider: VoiceProvider, model: String, voice: String): CallSession {
            started = Triple(provider, model, voice); starts++
            return CallSession.decode(Json.parseToJsonElement("""{"credential":"$credential","instructions":"You are the voice relay for Ada",
                "tools":[{"name":"send_to_bot","description":"Send","parameters":{"type":"object","properties":{"text":{"type":"string"}},"required":["text"]}},
                {"name":"bot_status","description":"Status","parameters":{"type":"object","properties":{}}}]}""").jsonObject).copy(googleSearch = googleSearch == true)
        }
        override suspend fun tool(name: String, arguments: JsonObject): JsonElement {
            tools += name to arguments
            return buildJsonObject { if (name == "bot_status") put("status", "idle") else put("sent", true) }
        }
        override fun add(from: String, piece: String) { lines += from to piece }
        override fun transcribing(on: Boolean) { }
        override fun endTurn() { turns++ }
        override fun reply(text: String) = "[Ada replied] ${SpokenText.from(text)}"
    }
    private fun entry(id: String, seq: Long, kind: String, data: JsonObjectBuilder.() -> Unit, bot: String = "ada") =
        Entry(id, seq, bot, seq, kind, data = buildJsonObject(data))

    @Test fun aCallsTranscriptJoinsEachTurnAndItsMessagesHideBehindItsLine() {
        var now = 0L
        val transcript = CallTranscript({ ++now }, { it.uppercase() }, { "l$now" }) { }
        transcript.add("user", "看一下 "); transcript.add("user", "git"); transcript.add("voice", " Checking.")
        assertEquals("the line being said stays", listOf(CallLine("user", "看一下 GIT", 1, "l1")), transcript.drain(withOpen = false))
        transcript.restore(listOf(CallLine("user", "AGAIN", 0)))
        assertEquals(listOf(CallLine("user", "AGAIN", 0), CallLine("voice", "CHECKING.", 2, "l2")), transcript.drain(withOpen = true))
        assertTrue(transcript.drain(withOpen = true).isEmpty())

        val line = entry("n1", 9, "notice", { put("text", "Git"); put("callSeconds", 26); put("call", "c")
            putJsonArray("transcript") { addJsonObject { put("from", "user"); put("text", "看一下 git"); put("at", 1) } } })
        val entries = listOf(
            entry("u1", 2, "user", { put("text", "check git"); put("call", "c"); put("from", "voice") }),
            entry("a1", 3, "agent", { put("text", "Clean"); put("final", true); put("call", "c") }),
            entry("u2", 4, "user", { put("text", "on this phone"); put("call", "live") }),
            entry("x1", 5, "user", { put("text", "typed") }), line,
        ).map { it.copy(createdAt = it.seq) }
        assertEquals(listOf("x1", "n1"), entries.hidingCalls().map(Entry::id))
        val file = entry("f1", 10, "agent", { put("text", "log.txt (3 bytes)"); put("final", true); put("call", "c")
            putJsonArray("files") { addJsonObject { put("id", "f"); put("name", "log.txt"); put("size", 3); put("sha256", "") } } })
        assertEquals("a file the call brought stays, to be opened", listOf("x1", "n1", "f1"), (entries + file).hidingCalls().map(Entry::id))
        assertEquals(listOf("User: 看一下 git", "VoiceToBot: check git", "Bot: Clean"),
            entries.callConversation(line).map { "${it.speaker}: ${it.text}" })
        // A call older than the chat the phone holds: its own entries, fetched, fill in the conversation once each.
        val old = listOf(line)
        assertTrue(old.callConversation(line).size == 1)
        val fetched = entries.filter { it.call == "c" && it.kind != "notice" }
        assertEquals(old.callConversation(line).size + 2, old.withCallEntries(fetched).callConversation(line).size)
        assertEquals(entries.size, entries.withCallEntries(fetched).size)
        val declarations = runBlocking { Link().start(VoiceProvider.Gemini, "m", "v") }.tools.map(CallTool::gemini)
        assertNull(declarations.first { it["name"]!!.jsonPrimitive.content == "bot_status" }["parameters"])
        assertEquals("STRING", declarations[0]["parameters"]!!.jsonObject["properties"]!!.jsonObject["text"]!!.jsonObject["type"]!!.jsonPrimitive.content)
    }

    private class Mic : MicSource {
        var frames: ((ByteArray, Float) -> Unit)? = null; var stops = 0; var refuse = false
        override suspend fun start(frames: (ByteArray, Float) -> Unit, failed: () -> Unit) { if (refuse) throw IOException("No microphone"); this.frames = frames }
        override suspend fun stop() { stops++ }
    }

    @Test fun cloudSpeechSendsEachUtteranceAndReadsRepliesClipByClip() = runTest {
        val mic = Mic(); val sent = mutableListOf<String>(); val heard = mutableListOf<Int>(); val made = mutableListOf<String>()
        val playing = mutableListOf<CompletableDeferred<Unit>>()
        val speech = CloudSpeechEngine.Speech({ heard += it.size; "  hello there " }, { made += it; it.toByteArray() to "aac" }, sent::add)
        val engine = CloudSpeechEngine(backgroundScope, VoiceProvider.Gemini, VoiceSettings(pauseMillis = 1_000), mic,
            { _, _ -> val clip = CompletableDeferred<Unit>().also { playing += it }
                try { clip.await() } catch (cancelled: CancellationException) { clip.cancel(); throw cancelled } }, speech) { testScheduler.currentTime }
        runCurrent(); assertEquals(VoicePhase.Starting, engine.state.value.phase) // Until the microphone delivers.
        val frame = ByteArray(3_200)
        repeat(6) { mic.frames!!(frame, .8f); advanceTimeBy(100); runCurrent() }
        mic.frames!!(frame, 0f); advanceTimeBy(1_100); runCurrent(); mic.frames!!(frame, 0f); runCurrent()
        assertEquals(listOf("hello there"), sent)
        val reply = "First sentence here. " + "x".repeat(200) + "."
        engine.speak("**$reply**"); runCurrent()
        assertEquals(VoicePhase.Speaking, engine.state.value.phase); assertEquals(2, made.size) // The next clip is fetched while one plays.
        mic.frames!!(frame, .9f); runCurrent(); assertEquals(0f, engine.state.value.level) // The mic doesn't count while it talks.
        playing[0].complete(Unit); runCurrent(); assertEquals(VoicePhase.Speaking, engine.state.value.phase)
        playing[1].complete(Unit); runCurrent(); assertEquals(VoicePhase.Speaking, engine.state.value.phase)
        playing[2].complete(Unit); runCurrent(); assertEquals(VoicePhase.Listening, engine.state.value.phase)
        engine.speak("Long reply."); runCurrent(); engine.interrupt(); runCurrent()
        assertEquals(VoicePhase.Listening, engine.state.value.phase); assertTrue(playing.last().isCancelled)
        engine.end(); runCurrent(); engine.awaitEnd()
        assertEquals(VoicePhase.Ended, engine.state.value.phase); assertEquals(1, mic.stops); assertFalse(engine.speak("Late"))
    }

    @Test fun aRejectedTranscriptionFailsTheCallWithTheProvidersReason() = runTest {
        val mic = Mic()
        val engine = CloudSpeechEngine(backgroundScope, VoiceProvider.OpenAI, VoiceSettings(pauseMillis = 1_000), mic, { _, _ -> },
            CloudSpeechEngine.Speech({ throw IOException("Incorrect API key") }, { error("unused") }) { fail("Nothing to send") }) { testScheduler.currentTime }
        runCurrent()
        repeat(6) { mic.frames!!(ByteArray(3_200), .8f); runCurrent() }
        mic.frames!!(ByteArray(3_200), 0f); advanceTimeBy(1_100); runCurrent(); mic.frames!!(ByteArray(3_200), 0f); runCurrent()
        engine.awaitEnd()
        assertEquals(VoicePhase.Failed, engine.state.value.phase); assertEquals("Incorrect API key", engine.state.value.error); assertEquals(1, mic.stops)
        val refused = CloudSpeechEngine(backgroundScope, VoiceProvider.OpenAI, VoiceSettings(), Mic().apply { refuse = true }, { _, _ -> },
            CloudSpeechEngine.Speech({ "" }, { error("unused") }) { })
        runCurrent(); assertEquals("No microphone", refused.state.value.error)
    }

    private class Peer : RealtimePeer {
        var events: RealtimePeer.Events? = null; var answer: String? = null; var micOn: Boolean? = null; var closed = 0
        override var isOpen = false
        val sent = mutableListOf<JsonObject>()
        override suspend fun offer(events: RealtimePeer.Events): String { this.events = events; return "offer-sdp" }
        override suspend fun answer(sdp: String) { answer = sdp }
        override fun send(text: String) { sent += Json.parseToJsonElement(text).jsonObject }
        override fun mic(enabled: Boolean) { micOn = enabled }
        override suspend fun close() { closed++ }
        fun types() = sent.map { it["type"]!!.jsonPrimitive.content }
    }

    @Test fun openAIRealtimeRelaysRepliesOneResponseAtATimeAndRunsTools() = runTest {
        val peer = Peer(); val link = Link(); var exchanged: Pair<String, String>? = null
        val engine = OpenAIRealtimeEngine(backgroundScope, VoiceSettings(), "gpt-realtime", "marin", "gpt-transcribe", link, peer) { secret, offer ->
            exchanged = secret to offer; "answer-sdp" }
        runCurrent()
        assertEquals("ek_secret" to "offer-sdp", exchanged); assertEquals("answer-sdp", peer.answer); assertEquals(true, peer.micOn)
        engine.speak("Early reply"); runCurrent(); assertTrue(peer.sent.isEmpty()) // Waits for the channel.
        peer.isOpen = true; peer.events!!.opened(); runCurrent()
        assertEquals(listOf("session.update"), peer.types()); assertEquals(VoicePhase.Starting, engine.state.value.phase)
        peer.events!!.received("""{"type":"session.updated"}"""); runCurrent()
        assertEquals(listOf("session.update", "conversation.item.create", "response.create"), peer.types())
        assertEquals("[Ada replied] Early reply", peer.sent[1]["item"]!!.jsonObject["content"]!!.jsonArray[0].jsonObject["text"]!!.jsonPrimitive.content)
        assertEquals(VoicePhase.Listening, engine.state.value.phase)
        peer.events!!.received("""{"type":"response.created"}"""); engine.announce("Approval waiting"); runCurrent()
        assertEquals(3, peer.sent.size)
        peer.events!!.received("""{"type":"response.function_call_arguments.done","call_id":"c1","name":"send_to_bot","arguments":"{\"text\":\"Ship it\"}"}""")
        peer.events!!.received("""{"type":"response.done"}"""); runCurrent()
        assertEquals(listOf("send_to_bot" to "Ship it"), link.tools.map { it.first to it.second["text"]!!.jsonPrimitive.content })
        assertEquals(listOf("function_call_output", "message"), peer.sent.drop(3).dropLast(1).map { it["item"]!!.jsonObject["type"]!!.jsonPrimitive.content })
        assertEquals("response.create", peer.types().last())
        peer.events!!.received("""{"type":"output_audio_buffer.started"}"""); runCurrent()
        assertEquals(VoicePhase.Speaking, engine.state.value.phase)
        engine.interrupt(); runCurrent()
        assertEquals(listOf("response.cancel", "output_audio_buffer.clear"), peer.types().takeLast(2))
        engine.mute(true); runCurrent(); assertEquals(false, peer.micOn)
        peer.events!!.ice(RealtimePeer.Ice.Disconnected); advanceTimeBy(4_000); runCurrent(); assertEquals(VoicePhase.Listening, engine.state.value.phase)
        peer.events!!.ice(RealtimePeer.Ice.Connected); advanceTimeBy(2_000); runCurrent(); assertEquals(VoicePhase.Listening, engine.state.value.phase)
        peer.events!!.ice(RealtimePeer.Ice.Failed); runCurrent()
        assertEquals(VoicePhase.Reconnecting, engine.state.value.phase); assertEquals(1, peer.closed) // Tried again, not hung up.
        engine.end(); engine.awaitEnd()
    }

    @Test fun openAIRealtimeReconnectsADroppedSessionPrimedByTheComputerAndGivesUpAfterThreeTries() = runTest {
        val peer = Peer(); val link = Link()
        var offers = 0
        val engine = OpenAIRealtimeEngine(backgroundScope, VoiceSettings(), "gpt-realtime", "marin", "gpt-transcribe", link, peer) { _, _ ->
            // OpenAI answers the first two sessions, then stops answering.
            if (++offers > 2) throw IOException("OpenAI answered 503."); "answer-sdp" }
        runCurrent(); peer.isOpen = true; peer.events!!.opened(); peer.events!!.received("""{"type":"session.updated"}"""); runCurrent()
        assertEquals(VoicePhase.Listening, engine.state.value.phase)
        peer.events!!.ice(RealtimePeer.Ice.Failed); runCurrent()
        assertEquals(VoicePhase.Reconnecting, engine.state.value.phase)
        engine.speak("Waits for the new session"); runCurrent()
        advanceTimeBy(Reconnect.delays[0] + 1); runCurrent()
        assertEquals(2, offers); assertEquals(2, link.starts) // A fresh secret and instructions from the computer.
        peer.events!!.opened(); peer.events!!.received("""{"type":"session.updated"}"""); runCurrent()
        assertEquals(VoicePhase.Listening, engine.state.value.phase)
        assertEquals("[Ada replied] Waits for the new session",
            peer.sent.last { it["type"]!!.jsonPrimitive.content == "conversation.item.create" }["item"]!!.jsonObject["content"]!!.jsonArray[0].jsonObject["text"]!!.jsonPrimitive.content)
        // A drop the new sessions never recover from: three tries, then the call fails over.
        peer.events!!.ice(RealtimePeer.Ice.Failed); runCurrent()
        repeat(Reconnect.delays.size) { advanceTimeBy(Reconnect.delays.max() + 15_001); runCurrent() }
        engine.awaitEnd()
        assertEquals(5, offers) // Each failed answer is a try of its own, not the end of the call.
        assertEquals(VoicePhase.Failed, engine.state.value.phase)
        assertEquals(VoiceFailureKind.Connection, engine.state.value.failureKind)
    }

    @Test fun callQualityReportsReadyTimeReconnectsStallsAndFailureOncePerEngine() {
        var now = 0L
        val reports = mutableListOf<Pair<String, CallQuality.Report>>()
        val quality = CallQuality<String>({ now }) { engine, report -> reports += engine to report }
        quality.begin("gemini")
        now = 1_200; quality.phase(VoicePhase.Listening, null)
        quality.phase(VoicePhase.Reconnecting, null); now = 2_000; quality.phase(VoicePhase.Listening, null)
        quality.stalled()
        quality.phase(VoicePhase.Failed, VoiceFailureKind.Connection)
        quality.begin("on_device")
        assertEquals("gemini" to CallQuality.Report(1_200, 1, VoiceStuck.Speaking, VoiceFailureKind.Connection), reports.single())
        now += CallQuality.STUCK_MILLIS; quality.report()
        assertEquals("on_device" to CallQuality.Report(null, 0, VoiceStuck.Connecting, null), reports.last())
    }

    @Test fun openAIRealtimeGivesUpWhenTheChannelNeverOpens() = runTest {
        val peer = Peer()
        val engine = OpenAIRealtimeEngine(backgroundScope, VoiceSettings(), "gpt-realtime", "marin", "gpt-transcribe", Link("ek"), peer) { _, _ -> "sdp" }
        advanceTimeBy(15_001); runCurrent(); engine.awaitEnd()
        assertEquals(VoicePhase.Failed, engine.state.value.phase); assertEquals("Couldn't reach OpenAI.", engine.state.value.error)
    }

    @Test fun openAIRealtimeStartTimerOutlastsIceUntilTheSessionIsSetUp() = runTest {
        val peer = Peer()
        val engine = OpenAIRealtimeEngine(backgroundScope, VoiceSettings(), "gpt-realtime", "marin", "gpt-transcribe", Link("ek"), peer) { _, _ -> "sdp" }
        runCurrent(); peer.isOpen = true; peer.events!!.opened()
        peer.events!!.ice(RealtimePeer.Ice.Disconnected); advanceTimeBy(5_001); runCurrent()
        assertEquals(VoicePhase.Starting, engine.state.value.phase) // No 5 s drop timer while starting.
        peer.events!!.ice(RealtimePeer.Ice.Connected); advanceTimeBy(10_000); runCurrent()
        assertEquals("Couldn't reach OpenAI.", engine.state.value.error) // ICE alone isn't connected.
        engine.awaitEnd()
    }

    @Test fun openAICallsEndpointPostsTheOfferAndExplainsRefusals() = runBlocking {
        MockWebServer().use { server ->
            server.enqueue(MockResponse().setBody("v=0 answer")); server.enqueue(MockResponse().setResponseCode(401).setBody("""{"error":{"message":"Invalid client secret"}}"""))
            val url = server.url("/v1/realtime/calls").toString()
            assertEquals("v=0 answer", OpenAIRealtimeEngine.answer(OkHttpClient(), "ek_1", "v=0 offer", url))
            val request = server.takeRequest()
            assertEquals("Bearer ek_1", request.getHeader("Authorization")); assertTrue(request.getHeader("Content-Type")!!.startsWith("application/sdp"))
            assertEquals("v=0 offer", request.body.readUtf8())
            assertEquals("Invalid client secret", runCatching { OpenAIRealtimeEngine.answer(OkHttpClient(), "ek_2", "offer", url) }.exceptionOrNull()?.message)
        }
    }

    private class Player : PcmPlayer {
        val chunks = LinkedBlockingQueue<() -> Unit>(); var stops = 0; var closed = 0
        override fun play(pcm: ByteArray, played: () -> Unit) { chunks += played }
        override fun stop() { stops++ }
        override suspend fun close() { closed++ }
    }

    @Test fun geminiLiveSetsUpTalksAndRunsToolsOverTheSocket() = geminiLiveWithSearch(null)
    @Test fun geminiLiveSearchKeepsTheBotTools() = geminiLiveWithSearch(true)
    @Test fun geminiLiveSearchStaysOffWhenDisabled() = geminiLiveWithSearch(false)

    private fun geminiLiveWithSearch(googleSearch: Boolean?) = runBlocking {
        val inbound = LinkedBlockingQueue<JsonObject>(); var server: WebSocket? = null
        val opened = CompletableDeferred<Unit>()
        MockWebServer().use { mock ->
            mock.enqueue(MockResponse().withWebSocketUpgrade(object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) { server = webSocket; opened.complete(Unit) }
                override fun onMessage(webSocket: WebSocket, text: String) { inbound += Json.parseToJsonElement(text).jsonObject }
            }))
            val dispatcher = Executors.newSingleThreadExecutor().asCoroutineDispatcher()
            val scope = CoroutineScope(SupervisorJob() + dispatcher)
            val mic = Mic(); val player = Player(); val link = Link("token/1", googleSearch)
            val endpoint = mock.url("/live").toString().replace("http://", "ws://")
            val engine = GeminiLiveEngine(scope, VoiceSettings(), "gemini-live-test", "Puck", link, mic, player,
                OkHttpClient(), endpoint)
            fun next() = requireNotNull(inbound.poll(5, TimeUnit.SECONDS)) { "Nothing arrived" }
            suspend fun until(check: (VoiceState) -> Boolean) = withTimeout(5_000) { engine.state.first(check) }
            assertEquals("/live?access_token=token%2F1", mock.takeRequest().path)
            withTimeout(5_000) { opened.await() }
            val setup = next()["setup"]!!.jsonObject
            assertEquals("models/gemini-live-test", setup["model"]!!.jsonPrimitive.content)
            assertEquals("Puck", setup["generationConfig"]!!.jsonObject["speechConfig"]!!.jsonObject["voiceConfig"]!!.jsonObject["prebuiltVoiceConfig"]!!.jsonObject["voiceName"]!!.jsonPrimitive.content)
            assertEquals(googleSearch == true, setup["tools"]!!.jsonArray.any { "googleSearch" in it.jsonObject })
            assertEquals(2, setup["tools"]!!.jsonArray[0].jsonObject["functionDeclarations"]!!.jsonArray.size)
            assertEquals("You are the voice relay for Ada", setup["systemInstruction"]!!.jsonObject["parts"]!!.jsonArray[0].jsonObject["text"]!!.jsonPrimitive.content)
            assertTrue("inputAudioTranscription" in setup && "outputAudioTranscription" in setup)
            engine.speak("Queued before setup")
            server!!.send("""{"setupComplete":{}}""")
            assertEquals("[Ada replied] Queued before setup", next()["clientContent"]!!.jsonObject["turns"]!!.jsonArray[0].jsonObject["parts"]!!.jsonArray[0].jsonObject["text"]!!.jsonPrimitive.content)
            assertEquals(VoicePhase.Starting, engine.state.value.phase) // Connecting until the microphone delivers.
            withTimeout(5_000) { while (mic.frames == null) delay(10) }
            mic.frames!!(byteArrayOf(1, 2), .5f)
            until { it.phase == VoicePhase.Listening }
            assertNull(engine.state.value.searchResult)
            server!!.send("""{"serverContent":{"outputTranscription":{"text":"A grounded answer."}}}""")
            server!!.send("""{"serverContent":{"groundingMetadata":{"searchEntryPoint":{"renderedContent":"<p>Search suggestions</p>"}},"turnComplete":true}}""")
            until { it.searchResult != null }
            assertEquals(CallSearchResult("<p>Search suggestions</p>", "A grounded answer."), engine.state.value.searchResult)
            server!!.send("""{"serverContent":{"outputTranscription":{"text":"Another answer"},"turnComplete":true}}""")
            until { it.searchResult == null }

            assertEquals("audio/pcm;rate=16000", next()["realtimeInput"]!!.jsonObject["audio"]!!.jsonObject["mimeType"]!!.jsonPrimitive.content)
            val audio = Base64.getEncoder().encodeToString(ByteArray(480))
            server!!.send("""{"serverContent":{"modelTurn":{"parts":[{"inlineData":{"mimeType":"audio/pcm;rate=24000","data":"$audio"}}]}}}""")
            until { it.phase == VoicePhase.Speaking }
            server!!.send("""{"serverContent":{"turnComplete":true}}""")
            requireNotNull(player.chunks.poll(5, TimeUnit.SECONDS))()
            until { it.phase == VoicePhase.Listening }
            server!!.send("""{"toolCall":{"functionCalls":[{"id":"f1","name":"bot_status","args":{}}]}}""")
            val response = next()["toolResponse"]!!.jsonObject["functionResponses"]!!.jsonArray[0].jsonObject
            assertEquals("f1", response["id"]!!.jsonPrimitive.content)
            assertEquals("idle", response["response"]!!.jsonObject["result"]!!.jsonObject["status"]!!.jsonPrimitive.content)
            server!!.close(1008, "API key not valid")
            until { it.phase == VoicePhase.Failed }
            assertEquals("API key not valid", engine.state.value.error)
            withTimeout(5_000) { engine.awaitEnd() }
            assertEquals(1, mic.stops); assertEquals(1, player.closed)
            scope.cancel(); dispatcher.close()
        }
    }

    @Test fun geminiLiveResumesADroppedSessionAndAStuckSpeakerGoesBackToListening() = runBlocking {
        val delays = Reconnect.delays; val stall = GeminiLiveEngine.stallMillis
        Reconnect.delays = listOf(20L, 20L, 20L); GeminiLiveEngine.stallMillis = 150L
        val inbound = LinkedBlockingQueue<Pair<Int, JsonObject>>(); val servers = LinkedBlockingQueue<WebSocket>()
        MockWebServer().use { mock ->
            repeat(2) { n ->
                mock.enqueue(MockResponse().withWebSocketUpgrade(object : WebSocketListener() {
                    override fun onOpen(webSocket: WebSocket, response: Response) { servers += webSocket }
                    override fun onMessage(webSocket: WebSocket, text: String) { inbound += n to Json.parseToJsonElement(text).jsonObject }
                    override fun onClosing(webSocket: WebSocket, code: Int, reason: String) { webSocket.close(1000, null) }
                }))
            }
            val dispatcher = Executors.newSingleThreadExecutor().asCoroutineDispatcher()
            val scope = CoroutineScope(SupervisorJob() + dispatcher)
            val mic = Mic(); val player = Player(); val link = Link("token")
            val engine = GeminiLiveEngine(scope, VoiceSettings(), "gemini-live-test", "Puck", link, mic, player, OkHttpClient(),
                mock.url("/live").toString().replace("http://", "ws://"))
            var stalls = 0
            engine.onStall = { stalls++ }
            fun next(key: String): Pair<Int, JsonObject> {
                while (true) { val m = requireNotNull(inbound.poll(5, TimeUnit.SECONDS)) { "No $key arrived" }; if (key in m.second) return m }
            }
            suspend fun until(check: (VoiceState) -> Boolean) = withTimeout(5_000) { engine.state.first(check) }
            next("setup")
            val first = requireNotNull(servers.poll(5, TimeUnit.SECONDS))
            first.send("""{"setupComplete":{}}""")
            withTimeout(5_000) { while (mic.frames == null) delay(10) }
            mic.frames!!(byteArrayOf(1, 2), .5f)
            until { it.phase == VoicePhase.Listening }
            first.send("""{"sessionResumptionUpdate":{"newHandle":"h1","resumable":true}}""")
            first.close(1011, "Internal error")
            until { it.phase == VoicePhase.Reconnecting }
            engine.speak("Said after the drop")
            val (session, setup) = next("setup")
            assertEquals(1, session); assertEquals(2, link.starts) // A fresh credential from the computer.
            assertEquals("h1", setup["setup"]!!.jsonObject["sessionResumption"]!!.jsonObject["handle"]!!.jsonPrimitive.content)
            val second = requireNotNull(servers.poll(5, TimeUnit.SECONDS))
            second.send("""{"setupComplete":{}}""")
            assertEquals(1, next("clientContent").first) // The reply that waited goes to the new session.
            assertEquals(VoicePhase.Reconnecting, engine.state.value.phase) // Ready again once the microphone delivers.
            mic.frames!!(byteArrayOf(1, 2), .5f)
            until { it.phase == VoicePhase.Listening }
            // The speaker never reports the chunk heard: the watchdog takes the call back to listening.
            val audio = Base64.getEncoder().encodeToString(ByteArray(480))
            second.send("""{"serverContent":{"modelTurn":{"parts":[{"inlineData":{"data":"$audio"}}]}}}""")
            until { it.phase == VoicePhase.Speaking }
            second.send("""{"serverContent":{"turnComplete":true}}""")
            until { it.phase == VoicePhase.Listening }
            assertEquals(1, stalls)
            engine.end(); withTimeout(5_000) { engine.awaitEnd() }
            scope.cancel(); dispatcher.close()
        }
        Reconnect.delays = delays; GeminiLiveEngine.stallMillis = stall
    }
}
