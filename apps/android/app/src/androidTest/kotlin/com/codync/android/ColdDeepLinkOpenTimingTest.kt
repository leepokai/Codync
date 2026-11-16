package com.codync.android

import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.junit4.createComposeRule
import com.codync.android.core.*
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class ColdDeepLinkOpenTimingTest {
    @get:Rule val compose = createComposeRule()

    @Test fun coldDeepLinkRecordsOpenChatOnlyAfterTheLaunchSplashLeaves() {
        val store = previewStore()
        val bot = Bot("deep-link-bot", "Scout")
        val state = AppState(previewComputer, loading = false, selectedBot = bot.id, bots = listOf(bot),
            connection = LinkState.Ready(HostRoute.Direct), actualConnection = LinkState.Ready(HostRoute.Direct),
            historyEnd = mapOf(bot.id to HistoryEnd.Complete), opensSeen = 1, catchUpsCompleted = 1,
            entries = listOf(Entry("deep-link-message", 1, bot.id, rev = 1, kind = "agent",
                data = buildJsonObject { put("text", "Ready."); put("final", true) })))
        val events = mutableListOf<PhonePerformance.OpenStep>()
        val timing = OpenTiming { events += it }
        val launching = mutableStateOf(true)
        compose.runOnUiThread {
            store.openTiming = timing
            timing.beginCold()
            timing.record(OpenPhase.CatchUp, previewComputer.id, chats = 0, messages = 0)
        }
        compose.setContent { Hosted { ChatScreen(state, store, editGroup = {}, launching = launching.value) } }
        compose.waitForIdle()
        assertEquals(0, events.count { it.phase == OpenPhase.OpenChat })

        compose.runOnUiThread { launching.value = false }
        compose.waitUntil(5_000) { events.count { it.phase == OpenPhase.OpenChat } == 1 }
        compose.waitForIdle()
        assertEquals(1, events.count { it.phase == OpenPhase.OpenChat })
    }
}
