package com.codync.android

import android.app.Application
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeUp
import androidx.compose.ui.unit.dp
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.HeldRosterHost
import java.util.UUID
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** The app's real channel: an open's first milestones and a real scroll reach the host. */
class PhonePerformanceDeliveryTest {
    @get:Rule val compose = createComposeRule()

    @Test fun anOpenAndAScrollReachTheHost() {
        val host = HeldRosterHost()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val local = LocalStore(context, "performance-delivery-" + UUID.randomUUID())
        val store = AppStore(context.applicationContext as Application, local, host.computer, "local", 0,
            SupervisorJob(), {}, {})
        // Emulators skip delivery unless a test asks; the host here is this test's own.
        deliverFromEmulator = true
        compose.setContent { Hosted {
            val list = rememberLazyListState()
            TrackScrollHitches(list, ScrollList.Bots) { store.recordPerformance(it) }
            LazyColumn(Modifier.fillMaxSize().testTag("bot-list"), state = list) {
                items(30) { Text("Bot $it", Modifier.height(56.dp)) }
            }
        } }
        try {
            runBlocking { store.open() }
            compose.runOnUiThread {
                store.openTiming = OpenTiming()
                store.openTiming.beginCold()
                store.setForeground(true)
            }
            compose.waitUntil(10_000) { host.openTimings().size == 2 }
            assertEquals(listOf("link_ready", "hello"), host.openTimings().map { it.phase() })

            compose.onNodeWithTag("bot-list").performTouchInput { swipeUp() }
            compose.waitUntil(10_000) { host.measurements("scroll").isNotEmpty() }
            val scroll = host.measurements("scroll").single()
            assertEquals("bots", scroll.getValue("list").jsonPrimitive.content)
            assertEquals(30, scroll.getValue("rows").jsonPrimitive.content.toInt())
            assertTrue(scroll.getValue("frames").jsonPrimitive.content.toInt() > 0)
        } finally {
            deliverFromEmulator = false
            compose.runOnUiThread { store.setForeground(false) }
            runBlocking { store.retire(unregister = false) }
            local.erase()
            host.close()
        }
    }

    private fun HeldRosterHost.measurements(type: String) =
        performances.filter { it["type"]?.jsonPrimitive?.content == type }

    private fun HeldRosterHost.openTimings() = measurements("open_timing")

    private fun JsonObject.phase() = getValue("phase").jsonPrimitive.content
}
