package com.codync.android

import android.app.NotificationManager
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.Lifecycle
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.util.UUID

/** Actual foreground audio and host transport; no real account or provider credentials. */
class LiveVoiceTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()
    @Test fun aCallSurvivesRotationAndBackgroundThenEndsFromItsNotification() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("liveVoice") == "true")
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assertNull(PushPreferences.activeUser(context))
        val local = LocalStore(context); val computer = local.computers().single()
        requireFixtureHost(computer.id)
        val channel = runBlocking { HostConnector.connect(computer.copy(urls = listOf("http://10.0.2.2:29322"), route = ConnectionRoute.DirectOnly), local.identity()) }
        val originalSettings = VoicePreferences.load(context)
        var botId: String? = null
        var model: AppStore? = null
        try {
            val parent = runBlocking { HostClient.bots(channel) }.single { it.name == "Android chat acceptance" }
            val source = WireJson.encodeToJsonElement(parent).jsonObject
            val draft = buildJsonObject {
                for (key in listOf("backend", "command", "cwd", "model")) source[key]?.let { put(key, it) }
                put("name", "Android voice acceptance " + UUID.randomUUID().toString().take(8))
                put("permission", "auto"); put("notify", false); put("connectors", JsonArray(emptyList())); put("skills", JsonArray(emptyList()))
            }
            val created = runBlocking { channel.call("createBot", draft) }
            val id = created.jsonObject.getValue("bot").jsonObject.getValue("id").jsonPrimitive.content
            botId = id
            val store = compose.computerStore(); model = store
            compose.waitUntil(30_000) { store.state.value.bots.any { it.id == id } && store.state.value.actualConnection is LinkState.Ready }
            compose.runOnUiThread { store.openBot(id) }
            compose.onNodeWithContentDescription("Call").performClick()
            compose.waitUntil(45_000) { store.state.value.voice?.state?.phase == VoicePhase.Listening || store.state.value.error != null }
            assertNull(store.state.value.error)
            val call = requireNotNull(store.state.value.voice)
            assertEquals("Offline English · Vosk", call.state.engine)
            compose.activityRule.scenario.recreate()
            compose.waitUntil(10_000) { store.state.value.voice?.id == call.id && compose.onAllNodesWithContentDescription("End call").fetchSemanticsNodes().size == 1 }
            compose.onNodeWithContentDescription("Call settings").performClick()
            compose.onNodeWithContentDescription("Send after a pause of:", substring = true).performClick()
            compose.onNodeWithText("1 s").performClick()
            compose.onNodeWithContentDescription("Reading speed:", substring = true).performClick()
            compose.onNodeWithText("Faster").performClick()
            compose.waitUntil { store.state.value.voice?.state?.settings == VoiceSettings(1_000, 1.12f) }
            InstrumentationRegistry.getInstrumentation().sendKeyDownUpSync(android.view.KeyEvent.KEYCODE_BACK)
            compose.waitUntil(5_000) { compose.onAllNodesWithText("Voice chat").fetchSemanticsNodes().isEmpty() }
            compose.onNodeWithContentDescription("Mute").performClick()
            compose.waitUntil { store.state.value.voice?.state?.muted == true }
            compose.activityRule.scenario.moveToState(Lifecycle.State.CREATED)
            runBlocking { delay(1_000); withTimeout(5_000) { store.installedConnectors() } }
            assertEquals(call.id, store.state.value.voice?.id)
            val notifications = context.getSystemService(NotificationManager::class.java)
            val active = notifications.activeNotifications.single { it.id == 308 }
            active.notification.actions.last().actionIntent.send()
            compose.waitUntil(10_000) { store.state.value.voice == null && notifications.activeNotifications.none { it.id == 308 } }
            runBlocking { withTimeout(10_000) {
                while (true) {
                    val history = channel.call("history", body("botId" to id))
                    val calls = history.jsonObject.getValue("entries").jsonArray.filter { it.jsonObject["data"]?.jsonObject?.get("callSeconds") != null }
                    if (calls.isNotEmpty()) {
                        assertEquals(1, calls.size)
                        assertTrue(calls.single().jsonObject.getValue("data").jsonObject.getValue("callSeconds").jsonPrimitive.long >= 1); break
                    }
                    delay(100)
                }
            } }
            compose.activityRule.scenario.moveToState(Lifecycle.State.RESUMED)
        } finally {
            compose.runOnUiThread { model?.endVoice(); model?.voiceSettings(originalSettings) }
            botId?.let { id -> runBlocking { channel.call("deleteBot", body("botId" to id)) } }
            channel.close()
        }
    }
}
