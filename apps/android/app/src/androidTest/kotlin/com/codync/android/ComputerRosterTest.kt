package com.codync.android

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ApplicationProvider
import com.codync.android.core.*
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** The roster's computer header: every paired computer to switch to, and the removed computer's page. */
class ComputerRosterTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()
    private val mac = Computer("AAAAAAAAAAAAAAAAAAAAAA", "Mac fixture", "mac")
    private val linux = Computer("BBBBBBBBBBBBBBBBBBBBBB", "Linux fixture", "linux")

    private fun state() = AppState(loading = false, rosterLoaded = true, computers = listOf(mac, linux),
        account = AccountState(ready = true), connection = LinkState.Ready(HostRoute.Direct),
        actualConnection = LinkState.Ready(HostRoute.Direct), bots = listOf(Bot("mac-bot", "Mac bot")))

    @Test fun theHeaderNamesTheShownComputerAndOffersTheOtherOne() {
        val store = AppStore(ApplicationProvider.getApplicationContext())
        var managed = false
        compose.setContent {
            Hosted {
                BotListView(state(), store, {}, { managed = true }, {}, {}, {}, 0.dp)
            }
        }
        compose.onNodeWithText("Mac bot").assertIsDisplayed()
        compose.onNode(hasContentDescription("Computers,", substring = true)).performClick()
        compose.onNodeWithText("Linux fixture").assertIsDisplayed()
        compose.onNodeWithText("Manage computers").performClick()
        assertTrue(managed)
    }

    @Test fun aRemovedComputersChatSaysSoUntilBack() {
        var back = false
        compose.setContent { Hosted { RemovedComputerView({ back = true }, {}) } }
        compose.onNodeWithText("This computer was removed").assertIsDisplayed()
        compose.onNodeWithContentDescription("Back").performClick()
        assertTrue(back)
    }
}
