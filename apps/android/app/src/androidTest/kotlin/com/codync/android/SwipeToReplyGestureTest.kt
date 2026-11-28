package com.codync.android

import android.graphics.Bitmap
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.material3.Text
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.hapticfeedback.HapticFeedback
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeUp
import androidx.compose.ui.unit.dp
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.Bot
import com.codync.android.core.Entry
import com.codync.android.design.CodyncTheme
import com.codync.android.design.OverlayRoot
import com.codync.android.design.Palette
import java.io.File
import kotlinx.serialization.json.add
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** A message pulled sideways (`SwipeToReply`): the reply opens on lift past the threshold, with one haptic on the way. */
class SwipeToReplyGestureTest {
    @get:Rule val compose = createComposeRule()
    private val haptics = mutableListOf<HapticFeedbackType>()
    private var replies = 0

    private fun show(direction: ReplyDirection) = compose.setContent {
        CompositionLocalProvider(LocalHapticFeedback provides object : HapticFeedback {
            override fun performHapticFeedback(hapticFeedbackType: HapticFeedbackType) { haptics += hapticFeedbackType }
        }) {
            SwipeToReply(direction, 40.dp, { replies++ }) { Text("Here is the plan.", Modifier.width(200.dp)) }
        }
    }

    /** Moves the finger by [dx] dp in one step, as a fast flick past the slop does. */
    private fun flick(dx: Float) = compose.onNodeWithText("Here is the plan.").performTouchInput {
        down(center)
        moveBy(Offset(dx.dp.toPx(), 0f))
        up()
    }

    @Test fun oneMovePastTheThresholdThenLiftReplies() {
        show(ReplyDirection.Right)
        flick(ReplySwipe.THRESHOLD_DP + 16f)
        compose.runOnIdle {
            assertEquals(1, replies)
            assertEquals(listOf(HapticFeedbackType.GestureThresholdActivate), haptics)
        }
    }

    @Test fun severalMovesPastTheThresholdThenLiftReplies() {
        show(ReplyDirection.Right)
        compose.onNodeWithText("Here is the plan.").performTouchInput {
            down(center)
            repeat(3) { moveBy(Offset(30.dp.toPx(), 0f)) }
            up()
        }
        compose.runOnIdle { assertEquals(1, replies) }
    }

    @Test fun aCancelledSwipeDoesNotReply() {
        show(ReplyDirection.Right)
        compose.onNodeWithText("Here is the plan.").performTouchInput {
            down(center)
            moveBy(Offset((ReplySwipe.THRESHOLD_DP + 16f).dp.toPx(), 0f))
            cancel()
        }
        compose.runOnIdle { assertEquals(0, replies) }
    }

    /** A tap starts a spring back of its own; the next swipe must still follow the finger, not the earlier one. */
    @Test fun aTapWhileSpringingBackKeepsTheNextSwipeFollowingTheFinger() {
        show(ReplyDirection.Right)
        compose.mainClock.autoAdvance = false
        flick(32f)
        compose.mainClock.advanceTimeByFrame()
        compose.onNodeWithText("Here is the plan.").performTouchInput {
            down(center)
            up()
        }
        compose.mainClock.advanceTimeByFrame()
        compose.onNodeWithText("Here is the plan.").performTouchInput {
            down(center)
            moveBy(Offset((ReplySwipe.THRESHOLD_DP + 16f).dp.toPx(), 0f))
        }
        compose.mainClock.advanceTimeByFrame()
        compose.onNodeWithText("Here is the plan.").performTouchInput { up() }
        compose.mainClock.autoAdvance = true
        compose.runOnIdle { assertEquals(1, replies) }
    }

    @Test fun aVoiceNotesSeekBarSeeksInsteadOfReplying() {
        var seek = 0f
        compose.setContent {
            SwipeToReply(ReplyDirection.Right, 40.dp, { replies++ }) {
                Box(Modifier.width(200.dp).pointerInput(Unit) {
                    detectHorizontalDragGestures { change, dragAmount -> change.consume(); seek += dragAmount }
                }) { Text("Here is the plan.") }
            }
        }
        flick(ReplySwipe.THRESHOLD_DP + 16f)
        compose.runOnIdle {
            assertEquals(0, replies)
            assertTrue(seek > 0f)
        }
    }

    @Test fun ownMessagePullsTheOtherWay() {
        show(ReplyDirection.Left)
        flick(-(ReplySwipe.THRESHOLD_DP + 16f))
        compose.runOnIdle { assertEquals(1, replies) }
    }

    @Test fun aVerticalDragScrollsTheMessageList() {
        compose.setContent {
            LazyColumn(Modifier.fillMaxSize()) {
                items(100) { index ->
                    SwipeToReply(ReplyDirection.Right, 40.dp, { replies++ }) { Text("Message $index") }
                }
            }
        }
        compose.onRoot().performTouchInput { swipeUp() }
        compose.onNodeWithText("Message 0").assertDoesNotExist()
        compose.runOnIdle { assertEquals(0, replies) }
    }

    @Test fun swipeEvidenceLight() = captureSwipeEvidence(false)

    @Test fun swipeEvidenceDark() = captureSwipeEvidence(true)

    private fun captureSwipeEvidence(dark: Boolean) {
        val group = Bot("swipe-preview", "Scout", kind = "group")
        val author = Bot("swipe-author", "Miles")
        val agentText = "Here is the plan."
        val userText = "Read me the spoken plan"
        val reactions = buildJsonArray { add("👍") }
        val agent = Entry("swipe-agent", 1, group.id, rev = 1, kind = "agent", createdAt = System.currentTimeMillis(),
            data = buildJsonObject {
                put("text", agentText); put("author", author.id); put("final", true); put("reactions", reactions)
            })
        val user = Entry("swipe-user", 2, group.id, rev = 1, kind = "user", createdAt = System.currentTimeMillis(),
            data = buildJsonObject { put("text", userText); put("reactions", reactions) })
        val state = AppState(previewComputer, loading = false, bots = listOf(group, author))
        val store = previewStore()
        compose.setContent {
            CodyncTheme(dark) { OverlayRoot {
                LazyColumn(Modifier.fillMaxSize().background(Palette.background).safeDrawingPadding().padding(16.dp)) {
                    item { ChatRow(agent, true, group, state, store, {}, { replies++ }, null,
                        ConnectionRequestActions({ _, _ -> }, { _, _ -> }), {}, {}) }
                    item { ChatRow(user, true, group, state, store, {}, { replies++ }, null,
                        ConnectionRequestActions({ _, _ -> }, { _, _ -> }), {}, {}) }
                }
            } }
        }
        compose.onNodeWithText(agentText).assertIsDisplayed()
        compose.onNodeWithText(userText).assertIsDisplayed()
        val appearance = if (dark) "dark" else "light"
        snapshot("swipe-to-reply-$appearance-rest")
        val agentMessage = compose.onNodeWithText(agentText)
        agentMessage.performTouchInput {
            down(center)
            moveBy(Offset(96.dp.toPx(), 0f))
        }
        snapshot("swipe-to-reply-$appearance-mid-drag")
        agentMessage.performTouchInput { up() }
        compose.runOnIdle { assertEquals(1, replies) }
        agentMessage.performTouchInput {
            down(center)
            moveBy(Offset(32.dp.toPx(), 0f))
            up()
        }
        compose.waitForIdle()
        snapshot("swipe-to-reply-$appearance-spring-back")
        val userMessage = compose.onNodeWithText(userText)
        userMessage.performTouchInput {
            down(center)
            moveBy(Offset(-96.dp.toPx(), 0f))
        }
        snapshot("swipe-to-reply-$appearance-own-mid-drag")
        userMessage.performTouchInput { up() }
        compose.runOnIdle { assertEquals(2, replies) }
        userMessage.performTouchInput {
            down(center)
            moveBy(Offset(-32.dp.toPx(), 0f))
            up()
        }
        compose.waitForIdle()
        snapshot("swipe-to-reply-$appearance-own-spring-back")
    }

    private fun snapshot(name: String) {
        if (InstrumentationRegistry.getArguments().getString("layoutSnapshots") != "true") return
        val screenshot = compose.onRoot().captureToImage().asAndroidBitmap()
        File(InstrumentationRegistry.getInstrumentation().targetContext.cacheDir, "$name.png").outputStream().use {
            screenshot.compress(Bitmap.CompressFormat.PNG, 100, it)
        }
    }

    @Test fun liftedShortOfTheThresholdNothingHappens() {
        show(ReplyDirection.Right)
        flick(ReplySwipe.THRESHOLD_DP / 2f)
        compose.runOnIdle {
            assertEquals(0, replies)
            assertEquals(emptyList<HapticFeedbackType>(), haptics)
        }
    }
}
