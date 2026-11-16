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
import com.codync.android.core.HostConnector
import com.codync.android.core.LinkState
import com.codync.android.core.Pairing
import java.util.Base64
import java.util.UUID
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

/**
 * The app's real store and channel against a real host, which logs what the phone sends: an open's first
 * milestones and a real scroll land in its `phone-performance.jsonl`. Driven by `host/tests/phone_performance_e2e.rs`,
 * which passes the host's pairing link, base64url-encoded, as the `pairing` instrumentation argument; skipped without it.
 */
class PhonePerformanceDeliveryTest {
    @get:Rule val compose = createComposeRule()

    @Test fun anOpenAndAScrollReachTheHost() {
        val encoded = InstrumentationRegistry.getArguments().getString("pairing")
        assumeTrue("needs a host's pairing link", encoded != null)
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val local = LocalStore(context, "performance-delivery-" + UUID.randomUUID())
        val link = String(Base64.getUrlDecoder().decode(encoded))
        val pairing = Pairing.parse(link, BuildConfig.URL_SCHEME).withoutRelay
        val computer = runBlocking { HostConnector.pair(pairing, local.identity(), "Performance test", null) }.computer
        val store = AppStore(context.applicationContext as Application, local, computer, "local", 0,
            SupervisorJob(), {}, {})
        // Emulators skip delivery unless a test asks; the host here is this test's own.
        deliverFromEmulator = true
        var scrolled = false
        compose.setContent { Hosted {
            val list = rememberLazyListState()
            TrackScrollHitches(list, ScrollList.Bots) { store.recordPerformance(it); scrolled = true }
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
            compose.waitUntil(20_000) { store.state.value.actualConnection is LinkState.Ready }
            compose.onNodeWithTag("bot-list").performTouchInput { swipeUp() }
            compose.waitUntil(10_000) { scrolled }
            // Delivery is fire-and-forget: let the sends reach the host, which checks its log once this test ends.
            Thread.sleep(500)
        } finally {
            deliverFromEmulator = false
            compose.runOnUiThread { store.setForeground(false) }
            runBlocking { store.retire(unregister = false) }
            local.erase()
        }
    }
}
