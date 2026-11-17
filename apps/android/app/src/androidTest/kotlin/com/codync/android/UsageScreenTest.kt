package com.codync.android

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.dp
import com.codync.android.core.*
import com.codync.android.design.LocalDismissModal
import kotlinx.coroutines.CompletableDeferred
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class UsageScreenTest {
    @get:Rule val compose = createComposeRule()

    private val computer = Computer("fixture", "Acceptance computer", "fixture")

    private fun usage(json: String) = WireJson.parseToJsonElement(json).jsonObject

    private fun offline(json: String) = AppState(computer, loading = false, usage = usage(json), actualConnection = LinkState.ComputerOffline(null))

    @Test fun unavailableWindowsAndOldReportsNeverBecomeZeroUsage() {
        val state = mutableStateOf(offline("""{"providers":[{"id":"future","name":"Future provider","source":"future-source","updatedAt":1,"windows":[{"id":"month","label":"Monthly"}]}]}"""))
        val refreshes = mutableListOf<Boolean>()
        var closed = false
        compose.setContent { Hosted { CompositionLocalProvider(LocalDismissModal provides { closed = true }) {
            UsageScreen(state.value, { refreshes.add(it) }, { refreshes.add(true) }, { _, _ -> }, {})
        } } }
        compose.onNodeWithText("Acceptance computer").assertIsDisplayed()
        compose.onNodeWithText("Future provider").assertIsDisplayed()
        compose.onNodeWithText("Monthly").assertIsDisplayed()
        compose.onNodeWithText("—").assertIsDisplayed()
        compose.onNodeWithText("0%").assertDoesNotExist()
        assertEquals(emptyList<Boolean>(), refreshes)
        // Coming online asks for a current report, from cache when the computer has one.
        compose.runOnUiThread { state.value = state.value.copy(actualConnection = LinkState.Ready(HostRoute.Direct)) }
        compose.waitForIdle()
        assertEquals(listOf(false), refreshes)
        compose.onNodeWithContentDescription("Close").performClick()
        assertTrue(closed)
    }

    @Test fun usageExplainsWhenThereIsNoComputerOrNoReportYet() {
        val state = mutableStateOf<AppState?>(null)
        compose.setContent { Hosted { UsageScreen(state.value, {}, {}, { _, _ -> }, {}) } }
        compose.onNodeWithText("No computer yet").assertIsDisplayed()
        compose.onNodeWithText("Usage shows up once a computer is connected.").assertIsDisplayed()
        compose.runOnUiThread { state.value = AppState(computer, loading = false) }
        compose.onNodeWithText("No usage information yet.").assertIsDisplayed()
    }

    @Test fun aLimitShowsItsPercentReadOutAsUsedAndANoteHasNoBar() {
        val state = offline("""{"providers":[{"id":"claude","name":"Claude","windows":[{"id":"week","label":"Weekly","percent":33},{"id":"cost","label":"Cost today","text":"$1.50"}]}]}""")
        compose.setContent { Hosted { UsageScreen(state, {}, {}, { _, _ -> }, {}) } }
        compose.onNodeWithText("33%").assertIsDisplayed()
        compose.onNodeWithContentDescription("33% used").assertIsDisplayed()
        compose.onNodeWithText("$1.50").assertIsDisplayed()
        compose.onNodeWithText("Cost today").assertIsDisplayed()
    }

    private val services = """"services":[{"id":"claude","name":"Claude","shown":true},{"id":"cursor","name":"Cursor","shown":false},{"id":"amp","name":"Amp","shown":true}]"""
    private val reading = """{"id":"claude","name":"Claude","windows":[{"id":"week","label":"Weekly","percent":10}]}"""

    @Test fun theButtonSwapsTheReadingsForTheAgentsAndATapReachesTheHost() {
        val changes = mutableListOf<Pair<String, Boolean>>()
        val state = offline("""{"providers":[$reading],$services}""")
        compose.setContent { Hosted { UsageScreen(state, {}, {}, { id, shown -> changes.add(id to shown) }, {}) } }
        compose.onNodeWithText("Weekly").assertIsDisplayed()
        compose.onNodeWithText("No reading yet").assertDoesNotExist()
        compose.onNodeWithContentDescription("Choose and arrange").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Weekly").assertDoesNotExist()
        compose.onNodeWithText("No reading yet").assertIsDisplayed()
        compose.onNodeWithText("On by default for the agents your bots run.").assertIsDisplayed()
        compose.onNodeWithText("Cursor").performClick()
        assertEquals(listOf("cursor" to true), changes)
        compose.onNodeWithContentDescription("Done").performClick()
        compose.waitForIdle()
        compose.onNodeWithText("Weekly").assertIsDisplayed()
    }

    @Test fun aDraggedRowHoldsItsPlaceUntilTheAnswerThenShowsTheHostsOrder() {
        val sent = mutableListOf<List<String>>()
        val answer = CompletableDeferred<Unit>()
        val state = offline("""{"providers":[$reading],$services}""")
        compose.setContent { Hosted { UsageScreen(state, {}, {}, { _, _ -> }, { sent.add(it); answer.await() }) } }
        compose.onNodeWithContentDescription("Choose and arrange").performClick()
        compose.waitForIdle()
        compose.onNodeWithContentDescription("Move Claude").performTouchInput {
            down(center)
            moveBy(Offset(0f, 60.dp.toPx()))
            up()
        }
        compose.waitForIdle()
        assertEquals(listOf(listOf("cursor", "claude", "amp")), sent)
        fun top(name: String) = compose.onNodeWithText(name).getBoundsInRoot().top
        assertTrue(top("Cursor") < top("Claude"))
        // This host keeps its order, as an older one refusing the call would.
        answer.complete(Unit)
        compose.waitForIdle()
        assertTrue(top("Claude") < top("Cursor"))
    }

    @Test fun anAccessibilityActionMovesTheRowAPlace() {
        val sent = mutableListOf<List<String>>()
        val state = offline("""{"providers":[$reading],$services}""")
        compose.setContent { Hosted { UsageScreen(state, {}, {}, { _, _ -> }, { sent.add(it) }) } }
        compose.onNodeWithContentDescription("Choose and arrange").performClick()
        compose.waitForIdle()
        val actions = compose.onNodeWithContentDescription("Move Cursor").fetchSemanticsNode().config[SemanticsActions.CustomActions]
        compose.runOnUiThread { actions.first { it.label == "Move up" }.action() }
        compose.waitForIdle()
        assertEquals(listOf(listOf("cursor", "claude", "amp")), sent)
    }

    @Test fun aHostWithNoListHasNoButton() {
        val state = AppState(computer, loading = false, usage = usage("""{"providers":[]}"""))
        compose.setContent { Hosted { UsageScreen(state, {}, {}, { _, _ -> }, {}) } }
        compose.onNodeWithText("No usage information yet.").assertIsDisplayed()
        compose.onNodeWithContentDescription("Choose and arrange").assertDoesNotExist()
    }
}
