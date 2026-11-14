package com.codync.android

import android.os.Build
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.Lifecycle
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import java.io.File
import java.util.UUID
import kotlinx.coroutines.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

/** The external emulator controller injects public fixture PCM, never a host microphone. */
class LiveVoiceSpeechTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    @Test fun repeatedBackgroundSpeechReachesTheHostAndFinalRepliesFinishPlayback() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("liveVoiceSpeech") == "true")
        assertTrue("Speech fixtures belong on emulators", Build.PRODUCT.contains("sdk"))
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        assertNull(PushPreferences.activeUser(context))
        val local = LocalStore(context); val computer = local.computers().single()
        requireFixtureHost(computer.id)
        val channel = runBlocking { HostConnector.connect(computer.copy(urls = listOf("http://10.0.2.2:29322"), route = ConnectionRoute.DirectOnly), local.identity()) }
        val marker = File(context.cacheDir, "voice-speech-stage")
        check(marker.parentFile!!.isDirectory || marker.parentFile!!.mkdirs())
        val observed = File(context.cacheDir, "voice-speech-observed")
        val originalSettings = VoicePreferences.load(context)
        var botId: String? = null; var model: AppStore? = null
        try {
            val parent = runBlocking { HostClient.bots(channel) }.single { it.name == "Android chat acceptance" }
            val source = WireJson.encodeToJsonElement(parent).jsonObject
            val draft = buildJsonObject {
                for (key in listOf("backend", "command", "cwd", "model")) source[key]?.let { put(key, it) }
                put("name", "Android speech acceptance " + UUID.randomUUID().toString().take(8))
                put("permission", "auto"); put("notify", false); put("connectors", JsonArray(emptyList())); put("skills", JsonArray(emptyList()))
            }
            val created = runBlocking { channel.call("createBot", draft) }
            val id = created.jsonObject.getValue("bot").jsonObject.getValue("id").jsonPrimitive.content
            botId = id
            val store = compose.computerStore(); model = store
            compose.waitUntil(30_000) { store.state.value.bots.any { it.id == id } && store.state.value.actualConnection is LinkState.Ready }
            compose.runOnUiThread { store.voiceSettings(VoiceSettings(2_500)); store.openBot(id) }
            compose.onNodeWithContentDescription("Call").performClick()
            compose.waitUntil(45_000) { store.state.value.voice?.state?.phase == VoicePhase.Listening || store.state.value.error != null }
            assertNull(store.state.value.error)
            val callId = requireNotNull(store.state.value.voice).id
            compose.activityRule.scenario.moveToState(Lifecycle.State.CREATED)
            for (turn in 1..2) {
                marker.writeText(turn.toString())
                runBlocking { withTimeout(60_000) {
                    while (true) {
                        val history = channel.call("history", body("botId" to id))
                        val entries = history.jsonObject.getValue("entries").jsonArray.map { Entry.decode(it.jsonObject) }
                        val users = entries.filter { it.kind == "user" && it.text.contains("one zero zero zero one") }
                        assertTrue("Recognition must not duplicate a turn", users.size <= turn)
                        val sent = users.getOrNull(turn - 1)
                        val reply = entries.firstOrNull { it.kind == "agent" && it.isChat && it.seq > (sent?.seq ?: Long.MAX_VALUE) }
                        val voice = store.state.value.voice
                        assertNotNull("Backgrounding must preserve the call", voice)
                        observed.writeText(buildJsonObject {
                            put("phase", voice?.state?.phase?.name); put("muted", voice?.state?.muted)
                            put("level", voice?.state?.level); put("heard", voice?.state?.heard)
                            put("said", voice?.state?.said); put("users", JsonArray(users.map { JsonPrimitive(it.text) }))
                            put("finals", entries.count { it.kind == "agent" && it.isChat })
                        }.toString())
                        if (sent != null && reply != null && voice?.state?.said == SpokenText.from(reply.text) && voice.state.phase == VoicePhase.Listening) {
                            assertEquals(callId, voice.id); break
                        }
                        assertNull(store.state.value.error)
                        delay(100)
                    }
                } }
            }
            compose.activityRule.scenario.moveToState(Lifecycle.State.RESUMED)
            compose.onNodeWithContentDescription("End call").performClick()
            compose.waitUntil(10_000) { store.state.value.voice == null }
        } finally {
            marker.delete(); observed.delete()
            compose.runOnUiThread { model?.endVoice(); model?.voiceSettings(originalSettings) }
            botId?.let { id -> runBlocking { channel.call("deleteBot", body("botId" to id)) } }
            channel.close()
        }
    }
}
