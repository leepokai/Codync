package com.codync.android

import androidx.activity.ComponentActivity
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.unit.dp
import com.codync.android.core.*
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class ConnectionUiTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()
    private val computer = Computer("fixture-computer-00001", "Fixture computer", "fixture")
    private val app = AccountContext(loading = false, setupComplete = true, account = AccountState(ready = true), computers = listOf(computer))
    private fun fixture(link: LinkState) = AppState(computer, loading = false, rosterLoaded = true, connection = link, actualConnection = link)

    @Test fun theComputerRowNamesTheActualConnectionState() = compose.withPreviewAccounts(computer) { accounts ->
        val state = mutableStateOf(fixture(LinkState.Failed("Your access was revoked.", 403)))
        compose.setContent { Hosted { ComputersView(app, listOf(state.value), accounts, {}, {}, {}) } }
        compose.onNodeWithText("Your access was revoked.").assertIsDisplayed()
        compose.onNodeWithText("Extensions").assertDoesNotExist()
        compose.runOnUiThread { state.value = fixture(LinkState.ComputerOffline(null)) }
        compose.onNodeWithText("Offline").assertIsDisplayed()
        compose.runOnUiThread { state.value = fixture(LinkState.Ready(HostRoute.Direct)).copy(bots = listOf(Bot("bot", "Saved bot"))) }
        compose.onNodeWithText("1 bot · Wi-Fi/Tailscale").assertIsDisplayed()
        compose.onNodeWithText("Extensions").assertIsDisplayed()
    }

    @Test fun cachedBotsStayListedWhileOfflineAndAnEmptyRosterOffersTheFirstBot() = compose.withPreviewAccounts(computer) { accounts ->
        val state = mutableStateOf(fixture(LinkState.ComputerOffline(null)).copy(bots = listOf(Bot("bot", "Saved bot"))))
        var created = false
        compose.setContent { Hosted { BotListView(app, listOf(state.value), accounts, {}, {}, { created = true }, {}, { _, _ -> }, {}, 0.dp) } }
        compose.onNodeWithText("Saved bot").assertIsDisplayed()
        compose.onNodeWithText("1 offline").assertIsDisplayed()
        compose.onNodeWithText("No bots yet").assertDoesNotExist()
        compose.runOnUiThread { state.value = fixture(LinkState.Ready(HostRoute.Direct)) }
        compose.onNodeWithText("1 connected").assertIsDisplayed()
        compose.onNodeWithText("No bots yet").assertIsDisplayed()
        compose.onNodeWithText("Create your first bot").performClick()
        assertTrue(created)
    }
}
