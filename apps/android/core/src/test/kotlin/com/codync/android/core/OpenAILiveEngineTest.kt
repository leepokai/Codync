package com.codync.android.core

import kotlinx.coroutines.*
import kotlinx.coroutines.test.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class OpenAILiveEngineTest {
    private class Peer : RealtimePeer {
        val listeners = mutableListOf<RealtimePeer.Events>()
        val sent = mutableListOf<JsonObject>()
        var audioWhenClosing: Pair<Boolean, Boolean>? = null
        var output = 0f; var speakerOn = true; var micOn = true; var answer: String? = null; var closes = 0
        override var isOpen = true
        override suspend fun offer(events: RealtimePeer.Events): String { listeners += events; return "offer" }
        override suspend fun answer(sdp: String) { answer = sdp }
        override fun send(text: String) {
            val event = WireJson.parseToJsonElement(text).jsonObject
            sent += event
            if (event.text("type") == "session.close") {
                audioWhenClosing = micOn to speakerOn
                listeners.last().received("{\"type\":\"session.closed\"}")
            }
        }
        override fun mic(enabled: Boolean) { micOn = enabled }
        override fun speaker(enabled: Boolean) { speakerOn = enabled }
        override suspend fun outputLevel() = output
        override suspend fun close() { closes++ }
        fun event(text: String) = listeners.last().received(text)
    }
    private class Link : LiveCallLink {
        val offers = mutableListOf<String>()
        val events = mutableListOf<Pair<String, JsonObject>>()
        val updates = mutableListOf<Triple<String, String, String>>()
        var response: CompletableDeferred<List<JsonObject>>? = null
        override suspend fun startLive(model: String, voice: String, sdp: String): LiveCallSession {
            offers += sdp
            return LiveCallSession("session-${offers.size}", "answer")
        }
        override suspend fun liveEvent(session: String, event: JsonObject): List<JsonObject> {
            events += session to event
            return response?.await() ?: listOf(buildJsonObject { put("type", "response.create") })
        }
        override suspend fun liveUpdate(session: String, kind: String, text: String): List<JsonObject> {
            updates += Triple(session, kind, text)
            return listOf(buildJsonObject { put("type", "session.commentary.append"); put("content", text); put("delegation_id", JsonNull) })
        }
    }

    @Test fun liveStartsThroughHostAndNeverUsesRealtimeSetup() = runTest {
        val peer = Peer(); val link = Link()
        val engine = OpenAILiveEngine(backgroundScope, VoiceSettings(), "gpt-live-1", "marin", link, peer)
        runCurrent()
        assertEquals(listOf("offer"), link.offers); assertEquals("answer", peer.answer)
        peer.listeners.last().opened(); runCurrent()
        assertTrue(peer.sent.isEmpty()); assertEquals(VoicePhase.Starting, engine.state.value.phase)
        engine.speak("Early reply"); runCurrent(); assertTrue(link.updates.isEmpty())
        peer.event("{\"type\":\"session.started\"}"); runCurrent()
        assertEquals(VoicePhase.Listening, engine.state.value.phase)
        assertEquals(Triple("session-1", "reply", "Early reply"), link.updates.single())
        assertEquals(listOf("session.commentary.append"), peer.sent.map { it.text("type") })
        engine.end(); engine.awaitEnd()
        assertEquals("session.close", peer.sent.last().text("type")); assertEquals(1, peer.closes)
        assertEquals(false to false, peer.audioWhenClosing)
    }

    @Test fun liveForwardsEventsInOrderAndTracksAudioInsteadOfTranscripts() = runTest {
        val peer = Peer(); val link = Link()
        val engine = OpenAILiveEngine(backgroundScope, VoiceSettings(), "gpt-live-1", "marin", link, peer)
        runCurrent(); peer.event("{\"type\":\"session.started\"}"); runCurrent()
        peer.event("{\"type\":\"session.output_transcript.delta\",\"delta\":\"hello\",\"start_ms\":0,\"end_ms\":30}")
        peer.event("{\"type\":\"response.event\",\"event\":{\"type\":\"response.output_item.done\"}}")
        runCurrent()
        assertEquals(listOf("session.output_transcript.delta", "response.event"), link.events.map { it.second.text("type") })
        assertEquals(VoicePhase.Listening, engine.state.value.phase)
        peer.output = .5f; advanceTimeBy(151); runCurrent(); assertEquals(VoicePhase.Speaking, engine.state.value.phase)
        engine.interrupt(); runCurrent(); assertFalse(peer.speakerOn)
        peer.event("{\"type\":\"session.input_transcript.delta\",\"delta\":\"wait\"}"); runCurrent(); assertTrue(peer.speakerOn)
        peer.output = 0f; advanceTimeBy(151); runCurrent(); assertEquals(VoicePhase.Listening, engine.state.value.phase)
        engine.end(); engine.awaitEnd()
    }

    @Test fun liveIgnoresOldChannelEventsAfterReconnect() = runTest {
        val peer = Peer(); val link = Link()
        val engine = OpenAILiveEngine(backgroundScope, VoiceSettings(), "gpt-live-1", "marin", link, peer)
        runCurrent(); peer.event("{\"type\":\"session.started\"}"); runCurrent()
        val old = peer.listeners.single()
        old.ice(RealtimePeer.Ice.Failed); runCurrent()
        assertEquals(VoicePhase.Reconnecting, engine.state.value.phase)
        advanceTimeBy(Reconnect.delays.first() + 1); runCurrent()
        peer.event("{\"type\":\"session.started\"}"); runCurrent()
        old.received("{\"type\":\"response.event\"}"); old.closed(); runCurrent()
        assertTrue(link.events.isEmpty()); assertEquals(VoicePhase.Listening, engine.state.value.phase)
        assertEquals(2, link.offers.size)
        engine.end(); engine.awaitEnd()
    }

    @Test fun liveDropsHostResultsAfterEndAndSurfacesProviderErrors() = runTest {
        val peer = Peer(); val link = Link()
        val engine = OpenAILiveEngine(backgroundScope, VoiceSettings(), "gpt-live-1", "marin", link, peer)
        runCurrent(); peer.event("{\"type\":\"session.started\"}"); runCurrent()
        link.response = CompletableDeferred()
        peer.event("{\"type\":\"response.event\"}"); runCurrent()
        engine.end(); engine.awaitEnd()
        link.response!!.complete(listOf(buildJsonObject { put("type", "response.create") })); runCurrent()
        assertEquals(listOf("session.close"), peer.sent.map { it.text("type") })
        val peer2 = Peer()
        val failed = OpenAILiveEngine(backgroundScope, VoiceSettings(), "gpt-live-1", "marin", Link(), peer2)
        runCurrent(); peer2.event("{\"type\":\"session.error\",\"error\":{\"message\":\"Model unavailable\"}}")
        runCurrent(); failed.awaitEnd()
        assertEquals(VoicePhase.Failed, failed.state.value.phase); assertEquals("Model unavailable", failed.state.value.error)
    }

    @Test fun liveSetupTimeoutSurvivesIceConnectedAndAudioFocusMutesBothDirections() = runTest {
        val peer = Peer()
        val engine = OpenAILiveEngine(backgroundScope, VoiceSettings(), "gpt-live-1", "marin", Link(), peer)
        runCurrent(); peer.listeners.single().ice(RealtimePeer.Ice.Connected)
        advanceTimeBy(15_001); runCurrent(); engine.awaitEnd()
        assertEquals(VoicePhase.Failed, engine.state.value.phase)
        val peer2 = Peer()
        val active = OpenAILiveEngine(backgroundScope, VoiceSettings(), "gpt-live-1", "marin", Link(), peer2)
        runCurrent(); peer2.event("{\"type\":\"session.started\"}"); runCurrent()
        active.focus(false); runCurrent(); assertFalse(peer2.micOn); assertFalse(peer2.speakerOn)
        active.focus(true); runCurrent(); assertTrue(peer2.micOn); assertTrue(peer2.speakerOn)
        active.end(); active.awaitEnd()
    }

    @Test fun oldHostsRequireUpdateBeforeLiveCanStartOrValidate() {
        for (status in listOf(buildJsonObject { }, buildJsonObject { put("supportsLive", false) })) {
            val failure = runCatching { requireOpenAILiveSupport(status) }.exceptionOrNull()
            assertEquals("Update Codync on the computer to use GPT-Live.", failure?.message)
        }
        requireOpenAILiveSupport(buildJsonObject { put("supportsLive", true) })
    }

    @Test fun onlyLiveConversationModelsUseLiveTransport() {
        assertTrue(isOpenAILiveModel("gpt-live-1")); assertTrue(isOpenAILiveModel("gpt-live-1.1"))
        assertFalse(isOpenAILiveModel("gpt-live-transcribe")); assertFalse(isOpenAILiveModel("gpt-realtime-2.1"))
        assertTrue(runCatching { LiveCallSession.decode(buildJsonObject { put("sessionId", ""); put("sdp", "offer") }) }.isFailure)
    }
}
