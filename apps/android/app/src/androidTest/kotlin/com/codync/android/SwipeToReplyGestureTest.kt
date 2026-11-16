package com.codync.android

import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.hapticfeedback.HapticFeedback
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
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

    @Test fun ownMessagePullsTheOtherWay() {
        show(ReplyDirection.Left)
        flick(-(ReplySwipe.THRESHOLD_DP + 16f))
        compose.runOnIdle { assertEquals(1, replies) }
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
