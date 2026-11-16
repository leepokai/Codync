package com.codync.android

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeUp
import androidx.compose.ui.unit.dp
import com.codync.android.core.Computer
import java.util.concurrent.CopyOnWriteArrayList
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** The bot list's scroll diagnostic follows the computer paired after the list first composed. */
class ScrollHitchFreshnessTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()
    private val computer = Computer("scroll-fixture-computer-01", "Fixture computer", "fixture")

    @Test fun aScrollAfterPairingIsReportedOnTheNewComputer() = compose.withPreviewAccounts { accounts ->
        // Setup skipped: the roster first composes without a current computer.
        val app = mutableStateOf(AccountContext(loading = false, setupComplete = true, account = AccountState(ready = true)))
        val reported = CopyOnWriteArrayList<Pair<String?, PhonePerformance.Scroll>>()
        compose.setContent { Hosted {
            val list = rememberLazyListState()
            val shown = app.value
            TrackScrollHitches(list, ScrollList.Bots) { scroll ->
                val store = accounts.store(shown.currentComputerId)
                store?.recordPerformance(scroll)
                reported.add(store?.id to scroll)
            }
            LazyColumn(Modifier.fillMaxSize().testTag("bot-list"), state = list) {
                items(30) { Text("Bot $it", Modifier.height(56.dp)) }
            }
        } }
        compose.runOnIdle { assertNull(app.value.currentComputerId) }

        // Pairing's observable effects: the first roster computer joins the account and its store opens.
        runBlocking(Dispatchers.Main) { accounts.open(computer) }
        try {
            compose.runOnUiThread { app.value = app.value.copy(computers = listOf(computer)) }
            compose.runOnIdle { assertEquals(computer.id, app.value.currentComputerId) }
            compose.onNodeWithTag("bot-list").performTouchInput { swipeUp() }
            compose.waitUntil(10_000) { reported.isNotEmpty() }
            compose.runOnIdle {
                val (computerId, scroll) = reported.single()
                assertEquals(computer.id, computerId)
                assertEquals(ScrollList.Bots, scroll.list)
                assertEquals(30, scroll.rows)
                assertTrue(scroll.frames > 0)
            }
        } finally { runBlocking(Dispatchers.Main) { accounts.close(computer.id) } }
    }
}
