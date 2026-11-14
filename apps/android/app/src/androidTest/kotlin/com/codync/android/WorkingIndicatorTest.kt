package com.codync.android

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.Entry
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Rule
import org.junit.Test

class WorkingIndicatorTest {
    @get:Rule val compose = createComposeRule()

    @Test fun stepsUnfoldFromTheActivity() {
        compose.mainClock.autoAdvance = false
        val step = Entry("tool", 0, "bot", 1, "tool", 1, buildJsonObject { put("title", "npm test"); put("status", "in_progress") })
        compose.setContent { Hosted { WorkingIndicator(Bot("bot", "Bot", status = "working", activity = "Terminal"), listOf(step), details = true) } }
        compose.mainClock.advanceTimeBy(1000)
        compose.onNodeWithText("npm test").assertDoesNotExist()
        compose.onNodeWithText("Terminal").performClick()
        compose.mainClock.advanceTimeBy(1000)
        compose.onNodeWithText("npm test").assertIsDisplayed()
        compose.onNodeWithText("Terminal").performClick()
        compose.mainClock.advanceTimeBy(1000)
        compose.onNodeWithText("npm test").assertDoesNotExist()
    }

    /** Claude's thinking text arrives as its thinking ends, so the activity unfolds before the turn has steps. */
    @Test fun activityUnfoldsBeforeTheTurnHasSteps() {
        compose.mainClock.autoAdvance = false
        compose.setContent { Hosted { WorkingIndicator(Bot("bot", "Bot", status = "working", activity = "Thinking…"), emptyList(), details = true) } }
        compose.mainClock.advanceTimeBy(1000)
        compose.onNodeWithText("Thinking…").performClick()
        compose.mainClock.advanceTimeBy(1000)
        compose.onNodeWithText("Nothing yet.").assertIsDisplayed()
    }

    /** Thinking details are off by default: the activity is a plain status line. */
    @Test fun withoutThinkingDetailsTheActivityDoesNotUnfold() {
        compose.mainClock.autoAdvance = false
        val step = Entry("tool", 0, "bot", 1, "tool", 1, buildJsonObject { put("title", "npm test"); put("status", "in_progress") })
        compose.setContent { Hosted { WorkingIndicator(Bot("bot", "Bot", status = "working", activity = "Terminal"), listOf(step), details = false) } }
        compose.mainClock.advanceTimeBy(1000)
        compose.onNodeWithText("Terminal").assertIsDisplayed().assertHasNoClickAction()
        compose.onNodeWithText("npm test").assertDoesNotExist()
    }

    @Test fun activityUpdatesWithoutMovingHistory() {
        // The live orb runs a frame loop; advance motion explicitly instead of waiting for it to stop.
        compose.mainClock.autoAdvance = false
        val activity = mutableStateOf("Working…")
        lateinit var list: LazyListState
        compose.setContent {
            Hosted {
                list = rememberLazyListState()
                rememberFollowing(list, "message", 1, working = true, bottom = 0.dp)
                LazyColumn(Modifier.size(320.dp).testTag("conversation"), state = list, contentPadding = PaddingValues(0.dp)) {
                    items(30) { Box(Modifier.height(48.dp)) { Text("Message $it") } }
                    item("working") { WorkingIndicator(Bot("bot", "Bot", status = "working", activity = activity.value), emptyList(), details = false) }
                }
            }
        }
        compose.mainClock.advanceTimeBy(1000)
        compose.waitForIdle()
        compose.onNodeWithText("Working…").assertIsDisplayed()
        compose.runOnIdle {
            assertFalse(list.canScrollForward)
            activity.value = "Searching files…"
        }
        compose.mainClock.advanceTimeBy(1000)
        compose.waitForIdle()
        compose.onNodeWithText("Searching files…").assertIsDisplayed()
        compose.runOnIdle { assertFalse(list.canScrollForward) }
        compose.onNodeWithTag("conversation").performTouchInput { swipeDown() }
        compose.mainClock.advanceTimeBy(1000)
        compose.waitForIdle()
        var place = 0 to 0
        compose.runOnIdle {
            place = list.firstVisibleItemIndex to list.firstVisibleItemScrollOffset
            activity.value = "Writing a reply…"
        }
        compose.mainClock.advanceTimeBy(1000)
        compose.waitForIdle()
        compose.runOnIdle { assertEquals(place, list.firstVisibleItemIndex to list.firstVisibleItemScrollOffset) }
    }
}
