package com.codync.android

import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.dp
import androidx.lifecycle.viewmodel.compose.viewModel
import com.codync.android.core.*
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class ConnectionUiTest {
    @get:Rule val compose = createComposeRule()
    private fun fixture(link: LinkState) = AppState(loading = false, setupComplete = true, rosterLoaded = true,
        computers = listOf(Computer("fixture", "Fixture computer", "fixture")), connection = link, actualConnection = link)

    @Test fun theComputerRowNamesTheActualConnectionState() {
        val state = mutableStateOf(fixture(LinkState.Failed("Your access was revoked.", 403)))
        compose.setContent { Hosted { ComputersView(state.value, viewModel(), {}, {}, {}) } }
        compose.onNodeWithText("Your access was revoked.").assertIsDisplayed()
        compose.onNodeWithText("Extensions").assertDoesNotExist()
        compose.runOnUiThread { state.value = fixture(LinkState.ComputerOffline(null)) }
        compose.onNodeWithText("Offline").assertIsDisplayed()
        compose.runOnUiThread { state.value = fixture(LinkState.Ready(HostRoute.Direct)).copy(bots = listOf(Bot("bot", "Saved bot"))) }
        compose.onNodeWithText("1 bot · Wi-Fi/Tailscale").assertIsDisplayed()
        compose.onNodeWithText("Extensions").assertIsDisplayed()
    }

    @Test fun cachedBotsStayListedWhileOfflineAndAnEmptyRosterOffersTheFirstBot() {
        val state = mutableStateOf(fixture(LinkState.ComputerOffline(null)).copy(bots = listOf(Bot("bot", "Saved bot"))))
        var created = false
        compose.setContent { Hosted { BotListView(state.value, viewModel(), {}, {}, { created = true }, {}, {}, 0.dp) } }
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
