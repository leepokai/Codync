package com.codync.android

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import com.codync.android.core.*
import com.codync.android.design.LocalDismissModal
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class UsageScreenTest {
    @get:Rule val compose = createComposeRule()

    @Test fun unavailableWindowsAndOldReportsNeverBecomeZeroUsage() {
        val raw = WireJson.parseToJsonElement("""{"providers":[{"id":"future","name":"Future provider","source":"future-source","updatedAt":1,"windows":[{"id":"month","label":"Monthly"}]}]}""")
        val state = mutableStateOf(AppState(Computer("fixture", "Acceptance computer", "fixture"), loading = false, usage = raw.jsonObject,
            actualConnection = LinkState.ComputerOffline(null)))
        val refreshes = mutableListOf<Boolean>()
        var closed = false
        compose.setContent { Hosted { CompositionLocalProvider(LocalDismissModal provides { closed = true }) {
            UsageScreen(state.value, { refreshes.add(it) }, { refreshes.add(true) }, { _, _ -> })
        } } }
        compose.onNodeWithText("Future provider").assertIsDisplayed()
        compose.onNodeWithText("Monthly").assertIsDisplayed()
        compose.onNodeWithText("—").assertIsDisplayed()
        compose.onNodeWithText("0%").assertDoesNotExist()
        compose.onNodeWithText("future-source").assertIsDisplayed()
        compose.onNodeWithText("Not reported").assertDoesNotExist()
        assertEquals(emptyList<Boolean>(), refreshes)
        compose.onNodeWithContentDescription("Collapse").performClick()
        compose.onNodeWithText("Monthly").assertDoesNotExist()
        compose.onNodeWithContentDescription("Expand").performClick()
        compose.onNodeWithText("Monthly").assertIsDisplayed()
        // Coming online asks for a current report, from cache when the computer has one.
        compose.runOnUiThread { state.value = state.value.copy(actualConnection = LinkState.Ready(HostRoute.Direct)) }
        compose.waitForIdle()
        assertEquals(listOf(false), refreshes)
        compose.onNodeWithContentDescription("Close").performClick()
        assertTrue(closed)
    }

    @Test fun usageExplainsWhenThereIsNoComputerOrNoReportYet() {
        val state = mutableStateOf<AppState?>(null)
        compose.setContent { Hosted { UsageScreen(state.value, {}, {}, { _, _ -> }) } }
        compose.onNodeWithText("No computer yet").assertIsDisplayed()
        compose.onNodeWithText("Usage shows up once a computer is connected.").assertIsDisplayed()
        compose.runOnUiThread { state.value = AppState(Computer("fixture", "Acceptance computer", "fixture"), loading = false) }
        compose.onNodeWithText("No usage yet").assertIsDisplayed()
        compose.onNodeWithText("Your computer reads plan limits from the agents your bots run and the ones you turn on.").assertIsDisplayed()
    }

    @Test fun theChooserListsTheAgentsAndATapReachesTheHostThenTheCardFoldsAway() {
        val usage = { providers: String, shown: Boolean ->
            WireJson.parseToJsonElement("""{"providers":[$providers],"services":[{"id":"claude","name":"Claude","shown":$shown},{"id":"cursor","name":"Cursor","shown":false},{"id":"amp","name":"Amp","shown":true}]}""").jsonObject
        }
        val card = """{"id":"claude","name":"Claude","source":"claude","updatedAt":1,"windows":[{"id":"week","label":"Weekly","percent":10}]}"""
        val computer = Computer("fixture", "Acceptance computer", "fixture")
        val state = mutableStateOf(AppState(computer, loading = false, usage = usage(card, true), actualConnection = LinkState.ComputerOffline(null)))
        val changes = mutableListOf<Pair<String, Boolean>>()
        compose.setContent { Hosted { UsageScreen(state.value, {}, {}, { id, shown -> changes.add(id to shown) }) } }
        compose.onNodeWithText("Show usage for").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("No reading yet").assertExists()
        compose.onNodeWithText("Cursor").performScrollTo().performClick()
        assertEquals(listOf("cursor" to true), changes)
        compose.onNodeWithText("Weekly").assertExists()
        compose.runOnUiThread { state.value = state.value.copy(usage = usage("", false)) }
        compose.waitForIdle()
        compose.onNodeWithText("Weekly").assertDoesNotExist()
    }

    @Test fun aHostWithNoListShowsNoSwitches() {
        val usage = WireJson.parseToJsonElement("""{"providers":[]}""").jsonObject
        val state = AppState(Computer("fixture", "Acceptance computer", "fixture"), loading = false, usage = usage)
        compose.setContent { Hosted { UsageScreen(state, {}, {}, { _, _ -> }) } }
        compose.onNodeWithText("No usage yet").assertIsDisplayed()
        compose.onNodeWithText("Show usage for").assertDoesNotExist()
    }
}
