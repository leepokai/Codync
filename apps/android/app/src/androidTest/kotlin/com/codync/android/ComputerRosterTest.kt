package com.codync.android

import androidx.activity.ComponentActivity
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.unit.dp
import com.codync.android.core.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/** Two computers in the roster: a heading each that folds, the header's filter, and removing one keeps the other. Nothing links. */
class ComputerRosterTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()
    private val mac = Computer("roster-test-mac-000001", "Mac fixture", "mac")
    private val linux = Computer("roster-test-linux-0001", "Linux fixture", "linux")
    // A ready account: one still loading shows a spinner in the header, and the roster never settles.
    private val app = AccountContext(loading = false, setupComplete = true, account = AccountState(ready = true), computers = listOf(mac, linux))
    private fun state(computer: Computer, bot: String) = AppState(computer, loading = false, rosterLoaded = true,
        connection = LinkState.Ready(HostRoute.Direct), actualConnection = LinkState.Ready(HostRoute.Direct), bots = listOf(Bot(bot, bot)))
    private fun heading(name: String) = hasContentDescription("$name,", substring = true) and isHeading()

    @Test fun eachComputerHasAHeadingThatFoldsAndTheFilterShowsSomeOrOne() = compose.withPreviewAccounts(mac, linux) { accounts ->
        val context = compose.activity
        val hidden = RosterComputers.hidden(context)
        val folded = RosterComputers.folded(context)
        RosterComputers.setHidden(context, ""); RosterComputers.setFolded(context, emptySet())
        try {
            compose.setContent {
                Hosted { BotListView(app, listOf(state(mac, "Mac bot"), state(linux, "Linux bot")), accounts, {}, {}, {}, {}, { _, _ -> }, {}, 0.dp) }
            }
            compose.onNode(heading("Mac fixture")).assertIsDisplayed()
            compose.onNode(heading("Linux fixture")).assertIsDisplayed()
            compose.onNodeWithText("2 connected").assertIsDisplayed()
            // A tap on a heading folds its bots away, and unfolds them again.
            compose.onNode(heading("Linux fixture")).performClick()
            compose.onNodeWithText("Linux bot").assertDoesNotExist()
            compose.onNodeWithText("Mac bot").assertIsDisplayed()
            compose.onNode(heading("Linux fixture")).performClick()
            compose.onNodeWithText("Linux bot").assertIsDisplayed()
            // Leaving one out shows the other as the only computer: no headings.
            compose.computerAction("Linux fixture")
            compose.onNodeWithText("Linux bot").assertDoesNotExist()
            compose.onNode(heading("Mac fixture")).assertDoesNotExist()
            compose.onNodeWithText("1 connected").assertIsDisplayed()
            compose.computerAction("All computers")
            compose.onNodeWithText("Linux bot").assertIsDisplayed()
            // Show only, from its submenu.
            compose.computerAction("Show only")
            compose.onNodeWithText("Linux fixture").performClick()
            compose.onNodeWithText("Mac bot").assertDoesNotExist()
            compose.onNodeWithText("Linux bot").assertIsDisplayed()
        } finally {
            RosterComputers.setHidden(context, hidden); RosterComputers.setFolded(context, folded)
        }
    }

    @Test fun removingOneComputerKeepsTheOther() = compose.withPreviewAccounts(mac, linux) { accounts ->
        runBlocking(Dispatchers.Main) { accounts.close(mac.id) }
        assertNull(accounts.store(mac.id))
        assertNotNull(accounts.store(linux.id))
    }

    @Test fun aChatWhoseComputerIsRemovedSaysSoUntilBack() = compose.withPreviewAccounts(mac, linux) { accounts ->
        runBlocking(Dispatchers.Main) {
            requireNotNull(accounts.store(linux.id)).openBot("Linux bot")
            accounts.close(linux.id)
        }
        assertTrue(accounts.state.value.removedChat)
        // The account's own state, as the app shows it: Back clears it there.
        compose.setContent {
            val live by accounts.state.collectAsState()
            Hosted { MainTabs(live, listOf(state(mac, "Mac bot")), accounts, {}, {}, {}, {}, {}, { _, _ -> }, {}, AppTab.Bots, {}, covered = true) {} }
        }
        compose.onNodeWithText("This computer was removed").assertIsDisplayed()
        compose.onNodeWithContentDescription("Back").performClick()
        compose.waitUntil(5_000) { !accounts.state.value.removedChat }
    }
}
