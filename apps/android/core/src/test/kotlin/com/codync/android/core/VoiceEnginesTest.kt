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

    private class Desk(override val bots: List<Bot>, override var entries: List<Entry>) : CallTarget {
        val sent = mutableListOf<String>(); val answered = mutableListOf<Pair<String, String>>()
        override fun send(text: String) { sent += text }
        override fun respond(entry: Entry, optionId: String) { answered += entry.id to optionId }
    }
    private fun entry(id: String, seq: Long, kind: String, data: JsonObjectBuilder.() -> Unit, bot: String = "ada", thread: String? = null) =
        Entry(id, seq, bot, thread, seq, kind, data = buildJsonObject(data))
    private val card = entry("card", 4, "permission", {
        put("status", "pending"); put("title", "Run npm test")
        putJsonArray("options") {
            addJsonObject { put("optionId", "allow_once"); put("name", "Allow once"); put("kind", "allow_once") }
            addJsonObject { put("optionId", "reject"); put("name", "Deny"); put("kind", "reject_once") }
        }
    })

    @Test fun theOperatorRelaysToTheBotAndAnswersApprovalsByName() {
        val desk = Desk(listOf(Bot("ada", "Ada", description = "web app", status = "working", activity = "Running tests")), listOf(
            entry("u", 1, "user", { put("text", "Run the tests") }),
            entry("a", 2, "agent", { put("text", "**Started** `npm test`"); put("final", true) }),
            entry("t", 3, "agent", { put("text", "In a thread"); put("final", true) }, thread = "root"), card))
        val operator = CallOperator("ada", desk)
        assertTrue(operator.instructions.startsWith("You are the voice on a phone call between the user and Ada (web app), a coding agent"))
        assertTrue("[Ada replied]" in operator.instructions)
        assertEquals("Nothing to send.", operator.call("send_to_bot", buildJsonObject { put("text", "  ") })["error"]?.jsonPrimitive?.content)
        operator.call("send_to_bot", buildJsonObject { put("text", " Fix the build ") }); assertEquals(listOf("Fix the build"), desk.sent)
        val status = operator.call("bot_status", JsonObject(emptyMap()))
        assertEquals("Run npm test", status["approval"]!!.jsonObject["request"]!!.jsonPrimitive.content)
        val messages = operator.call("recent_messages", buildJsonObject { put("count", 2.0) })["messages"]!!.jsonArray
        assertEquals(listOf("ada" to "Started npm test", "approval request" to "Run npm test"),
            messages.map { it.jsonObject["from"]!!.jsonPrimitive.content.lowercase() to it.jsonObject["text"]!!.jsonPrimitive.content })
        assertEquals("Allow once", operator.call("answer_approval", buildJsonObject { put("option", "allow") })["answered"]?.jsonPrimitive?.content)
        assertEquals(listOf("card" to "allow_once"), desk.answered)
        assertEquals(2, operator.call("answer_approval", buildJsonObject { put("option", "maybe") })["options"]!!.jsonArray.size)
        assertEquals("Unknown tool shell.", operator.call("shell", JsonObject(emptyMap()))["error"]?.jsonPrimitive?.content)
        assertEquals("[Ada replied] Done", operator.reply("**Done**")); assertEquals("[Notice] Hi", operator.notice("Hi"))
        val group = CallOperator("team", Desk(listOf(Bot("team", "Team", kind = "group", members = listOf("ada", "bo")), Bot("ada", "Ada"), Bot("bo", "Bo")), emptyList()))
        assertTrue("the group chat Team on the user's computer, where the bots Ada, Bo answer in turn" in group.instructions)
        val declarations = CallOperator.tools.map(CallTool::gemini)
        assertNull(declarations.first { it["name"]!!.jsonPrimitive.content == "bot_status" }["parameters"])
        assertEquals("OBJECT", declarations[0]["parameters"]!!.jsonObject["type"]!!.jsonPrimitive.content)
        assertEquals("STRING", declarations[0]["parameters"]!!.jsonObject["properties"]!!.jsonObject["text"]!!.jsonObject["type"]!!.jsonPrimitive.content)
        assertEquals("function", CallOperator.tools[0].openAI()["type"]!!.jsonPrimitive.content)
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
        runCurrent(); assertEquals(VoicePhase.Listening, engine.state.value.phase)
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
        val peer = Peer(); val desk = Desk(listOf(Bot("ada", "Ada")), emptyList()); var exchanged: Pair<String, String>? = null
        val engine = OpenAIRealtimeEngine(backgroundScope, VoiceSettings(), { "ek_secret" }, CallOperator("ada", desk), peer) { secret, offer ->
            exchanged = secret to offer; "answer-sdp" }
        runCurrent()
        assertEquals("ek_secret" to "offer-sdp", exchanged); assertEquals("answer-sdp", peer.answer); assertEquals(true, peer.micOn)
        engine.speak("Early reply"); runCurrent(); assertTrue(peer.sent.isEmpty()) // Waits for the channel.
        peer.isOpen = true; peer.events!!.opened(); runCurrent()
        assertEquals(listOf("session.update", "conversation.item.create", "response.create"), peer.types())
        assertEquals("[Ada replied] Early reply", peer.sent[1]["item"]!!.jsonObject["content"]!!.jsonArray[0].jsonObject["text"]!!.jsonPrimitive.content)
        assertEquals(VoicePhase.Listening, engine.state.value.phase)
        peer.events!!.received("""{"type":"response.created"}"""); engine.announce("Approval waiting"); runCurrent()
        assertEquals(3, peer.sent.size)
        peer.events!!.received("""{"type":"response.function_call_arguments.done","call_id":"c1","name":"send_to_bot","arguments":"{\"text\":\"Ship it\"}"}""")
        peer.events!!.received("""{"type":"response.done"}"""); runCurrent()
        assertEquals(listOf("Ship it"), desk.sent)
        assertEquals(listOf("function_call_output", "message"), peer.sent.drop(3).dropLast(1).map { it["item"]!!.jsonObject["type"]!!.jsonPrimitive.content })
        assertEquals("response.create", peer.types().last())
        peer.events!!.received("""{"type":"output_audio_buffer.started"}"""); runCurrent()
        assertEquals(VoicePhase.Speaking, engine.state.value.phase)
        engine.interrupt(); runCurrent()
        assertEquals(listOf("response.cancel", "output_audio_buffer.clear"), peer.types().takeLast(2))
        engine.mute(true); runCurrent(); assertEquals(false, peer.micOn)
        peer.events!!.ice(RealtimePeer.Ice.Disconnected); advanceTimeBy(4_000); runCurrent(); assertEquals(VoicePhase.Listening, engine.state.value.phase)
        peer.events!!.ice(RealtimePeer.Ice.Connected); advanceTimeBy(2_000); runCurrent(); assertEquals(VoicePhase.Listening, engine.state.value.phase)
        peer.events!!.ice(RealtimePeer.Ice.Failed); runCurrent(); engine.awaitEnd()
        assertEquals("Lost the connection to OpenAI.", engine.state.value.error); assertEquals(1, peer.closed)
    }

    @Test fun openAIRealtimeGivesUpWhenTheChannelNeverOpens() = runTest {
        val peer = Peer()
        val engine = OpenAIRealtimeEngine(backgroundScope, VoiceSettings(), { "ek" }, CallOperator("ada", Desk(emptyList(), emptyList())), peer) { _, _ -> "sdp" }
        advanceTimeBy(15_001); runCurrent(); engine.awaitEnd()
        assertEquals(VoicePhase.Failed, engine.state.value.phase); assertEquals("Couldn't reach OpenAI.", engine.state.value.error)
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

    @Test fun geminiLiveSetsUpTalksAndRunsToolsOverTheSocket() = runBlocking {
        val inbound = LinkedBlockingQueue<JsonObject>(); var server: WebSocket? = null
        val opened = CompletableDeferred<Unit>()
        MockWebServer().use { mock ->
            mock.enqueue(MockResponse().withWebSocketUpgrade(object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) { server = webSocket; opened.complete(Unit) }
                override fun onMessage(webSocket: WebSocket, text: String) { inbound += Json.parseToJsonElement(text).jsonObject }
            }))
            val dispatcher = Executors.newSingleThreadExecutor().asCoroutineDispatcher()
            val scope = CoroutineScope(SupervisorJob() + dispatcher)
            val mic = Mic(); val player = Player(); val desk = Desk(listOf(Bot("ada", "Ada", status = "idle")), emptyList())
            val endpoint = mock.url("/live").toString().replace("http://", "ws://")
            val engine = GeminiLiveEngine(scope, VoiceSettings(), "gemini-live-test", "Puck", { "token/1" }, CallOperator("ada", desk), mic, player,
                OkHttpClient(), endpoint)
            fun next() = requireNotNull(inbound.poll(5, TimeUnit.SECONDS)) { "Nothing arrived" }
            suspend fun until(check: (VoiceState) -> Boolean) = withTimeout(5_000) { engine.state.first(check) }
            assertEquals("/live?access_token=token%2F1", mock.takeRequest().path)
            withTimeout(5_000) { opened.await() }
            val setup = next()["setup"]!!.jsonObject
            assertEquals("models/gemini-live-test", setup["model"]!!.jsonPrimitive.content)
            assertEquals("Puck", setup["generationConfig"]!!.jsonObject["speechConfig"]!!.jsonObject["voiceConfig"]!!.jsonObject["prebuiltVoiceConfig"]!!.jsonObject["voiceName"]!!.jsonPrimitive.content)
            assertEquals(4, setup["tools"]!!.jsonArray[0].jsonObject["functionDeclarations"]!!.jsonArray.size)
            engine.speak("Queued before setup")
            server!!.send("""{"setupComplete":{}}""")
            until { it.phase == VoicePhase.Listening }
            assertEquals("[Ada replied] Queued before setup", next()["clientContent"]!!.jsonObject["turns"]!!.jsonArray[0].jsonObject["parts"]!!.jsonArray[0].jsonObject["text"]!!.jsonPrimitive.content)
            mic.frames!!(byteArrayOf(1, 2), .5f)
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
}
