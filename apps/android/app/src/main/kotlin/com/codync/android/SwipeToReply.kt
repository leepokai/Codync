package com.codync.android

import androidx.compose.animation.core.animate
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.changedToUp
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.design.Motion
import com.codync.android.design.Palette
import com.codync.android.design.Symbol
import kotlin.math.abs
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch

/** The arithmetic of a sideways pull on a message bubble (iMessage; iOS `Chat/SwipeToReply.swift`). */
internal object ReplySwipe {
    /** The pull that opens a reply, dp. */
    const val THRESHOLD_DP = 64f

    /** How far the bubble follows [travel] in the swipe's direction: freely to [threshold], a quarter past it. */
    fun offset(travel: Float, threshold: Float): Float =
        if (travel <= 0f) 0f else if (travel <= threshold) travel else threshold + (travel - threshold) * 0.25f

    /** The visible movement stays in the open [gap] beside the message, including its margin; full offset decides the reply. */
    fun shown(offset: Float, gap: Float): Float = offset.coerceAtMost(gap.coerceAtLeast(0f))

    /** The reply arrow's grow and fade over a pull, full at [threshold]. */
    fun progress(offset: Float, threshold: Float): Float = (offset / threshold).coerceIn(0f, 1f)
}

/** Which way a row's bubble can be pulled to reply: a bot's message to the right, one's own to the left. */
internal enum class ReplyDirection(val sign: Float) {
    Right(1f), Left(-1f);

    val edge: Alignment get() = if (this == Right) Alignment.CenterStart else Alignment.CenterEnd
}

/**
 * Dragging a message sideways starts a reply to it: [content] follows the finger toward [direction]'s side,
 * the reply arrow growing in the gap it leaves. Letting go at the threshold replies and counts; short of it,
 * or a cancelled touch, the row springs back. Without [reply] this is the content alone.
 */
@Composable internal fun SwipeToReply(direction: ReplyDirection, margin: Dp, reply: (() -> Unit)?, content: @Composable () -> Unit) {
    if (reply == null) {
        Box(Modifier.fillMaxWidth(), contentAlignment = direction.edge) { content() }
        return
    }
    val density = LocalDensity.current
    val threshold = with(density) { ReplySwipe.THRESHOLD_DP.dp.toPx() }
    val marginPx = with(density) { margin.toPx() }
    val haptics = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    val action by rememberUpdatedState(reply)
    var offset by remember { mutableFloatStateOf(0f) }
    var gap by remember { mutableFloatStateOf(0f) }
    var settling by remember { mutableStateOf<Job?>(null) }
    fun settle() {
        // Only one spring back runs: a tap while the last one is still moving starts from where it is.
        settling?.cancel()
        settling = scope.launch {
            animate(ReplySwipe.shown(offset, gap), 0f, animationSpec = Motion.layout()) { value, _ -> offset = value }
        }
    }
    Box(Modifier.fillMaxWidth()) {
        // Drawn under the content, so the moving bubble uncovers the arrow in the gap it leaves.
        Box(Modifier.align(direction.edge)
            .width(with(density) { ReplySwipe.shown(offset, gap).toDp() }).clipToBounds().graphicsLayer {
                val progress = ReplySwipe.progress(offset, threshold)
                alpha = progress
                val scale = 0.5f + 0.5f * progress
                scaleX = scale; scaleY = scale
            }, contentAlignment = Alignment.Center) {
            Symbol("arrowshape.turn.up.left", size = 20.dp, tint = Palette.secondary)
        }
        Box(Modifier.align(direction.edge)
            .onGloballyPositioned { coordinates ->
                gap = coordinates.parentLayoutCoordinates?.let { (it.size.width - coordinates.size.width).toFloat() + marginPx } ?: 0f
            }.pointerInput(direction, density, marginPx, haptics) {
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false, pass = PointerEventPass.Main)
                    try {
                        var dx = 0f
                        var dy = 0f
                        // The swipe takes the pointer only once horizontal travel beats the slop in the allowed direction
                        // before a mostly-vertical drag would: the list's scroll and taps hold still keep working. Listen on
                        // the Main pass so a voice note's seek bar can consume its own drag before this row accepts it.
                        while (true) {
                            val change = awaitPointerEvent(PointerEventPass.Main).changes.firstOrNull { it.id == down.id } ?: return@awaitEachGesture
                            if (change.isConsumed || !change.pressed) return@awaitEachGesture
                            dx += change.positionChange().x
                            dy += change.positionChange().y
                            if (abs(dx) > viewConfiguration.touchSlop && abs(dx) > abs(dy)) {
                                if (dx * direction.sign > 0f) { change.consume(); settling?.cancel(); break }
                                return@awaitEachGesture
                            }
                            if (abs(dy) > viewConfiguration.touchSlop && abs(dy) >= abs(dx)) return@awaitEachGesture
                        }
                        var travel = 0f
                        var past = false
                        fun pull(delta: Float) {
                            travel += delta * direction.sign
                            offset = ReplySwipe.offset(travel, threshold)
                            val now = offset >= threshold
                            if (now != past) {
                                past = now
                                if (now) haptics.performHapticFeedback(HapticFeedbackType.GestureThresholdActivate)
                            }
                        }
                        // The move that crossed the slop already counts, so a lift right after it still replies.
                        pull(dx)
                        var lifted = false
                        while (true) {
                            val change = awaitPointerEvent(PointerEventPass.Main).changes.firstOrNull { it.id == down.id } ?: break
                            if (change.changedToUp()) { change.consume(); lifted = true; break }
                            if (change.isConsumed || !change.pressed) break
                            pull(change.positionChange().x)
                            change.consume()
                        }
                        if (lifted && offset >= threshold) {
                            Analytics.capture(AnalyticsEvent.ReplySwiped)
                            action()
                        }
                    } finally {
                        // Android turns ACTION_CANCEL into coroutine cancellation; settle there too.
                        settle()
                    }
                }
            }.graphicsLayer { translationX = direction.sign * ReplySwipe.shown(offset, gap) }) { content() }
    }
}
