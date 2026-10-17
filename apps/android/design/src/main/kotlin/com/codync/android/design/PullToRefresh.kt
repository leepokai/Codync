package com.codync.android.design

import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animate
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
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
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.clipPath
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.dp
import kotlin.math.hypot
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** How far past the top a pull completes the circle. */
private val PullDistance = 110.dp
/** Room kept open above the content while it refreshes, for the indicator. */
private val PullHold = 52.dp
/** A finger past the top moves the content this much (UIKit's rubber band, roughly). */
private const val PullResistance = 0.5f
/** Long enough to read as a refresh even when it's done at once. */
private const val MinimumRefreshMillis = 600L

/**
 * A pull past the top of scrolling content (iOS `PullToRefresh` with its chat list): the content
 * follows the finger, the arrow sweeps in with the pull, and the refresh starts only when the
 * reader lets go of a full circle, so a long pull never refreshes under the finger. The content
 * stays lowered under the turning arrow until `refresh` returns, then eases back up.
 * `indicatorTop` is where the content's visible top is (under any bars it scrolls beneath).
 */
@Composable fun PullToRefresh(refresh: suspend () -> Unit, modifier: Modifier = Modifier, indicatorTop: Dp = 0.dp,
    content: @Composable BoxScope.() -> Unit) {
    val density = LocalDensity.current
    val distance = with(density) { PullDistance.toPx() }
    val hold = with(density) { PullHold.toPx() }
    val haptics = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    val latest by rememberUpdatedState(refresh)
    var pull by remember { mutableFloatStateOf(0f) }
    var refreshing by remember { mutableStateOf(false) }
    val connection = remember(distance, hold) {
        object : NestedScrollConnection {
            var settling: Job? = null

            fun pullBy(delta: Float) {
                settling?.cancel()
                val before = pull
                pull = (pull + delta).coerceAtLeast(0f)
                if (before < distance && pull >= distance) haptics.performHapticFeedback(HapticFeedbackType.GestureThresholdActivate)
            }

            fun settle(to: Float) {
                settling = scope.launch { animate(pull, to, animationSpec = Motion.spring(0.5f)) { value, _ -> pull = value } }
            }

            // Pushing back up takes back the pull before the content scrolls.
            override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                if (refreshing || pull <= 0f || available.y >= 0f || source != NestedScrollSource.UserInput) return Offset.Zero
                val used = maxOf(available.y, -pull / PullResistance)
                pullBy(used * PullResistance)
                return Offset(0f, used)
            }

            override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
                if (refreshing || available.y <= 0f || source != NestedScrollSource.UserInput) return Offset.Zero
                pullBy(available.y * PullResistance)
                return Offset(0f, available.y)
            }

            override suspend fun onPreFling(available: Velocity): Velocity {
                if (refreshing || pull <= 0f) return Velocity.Zero
                if (pull < distance) { settle(0f); return available }
                refreshing = true
                settle(hold)
                scope.launch {
                    val started = System.currentTimeMillis()
                    try { latest() }
                    finally {
                        delay(MinimumRefreshMillis - (System.currentTimeMillis() - started))
                        refreshing = false
                        settle(0f)
                    }
                }
                return available
            }
        }
    }
    Box(modifier.nestedScroll(connection)) {
        Box(Modifier.fillMaxSize().graphicsLayer { translationY = pull }, content = content)
        PullIndicator(if (refreshing) 1f else (pull / distance).coerceIn(0f, 1f), refreshing,
            Modifier.align(Alignment.TopCenter).padding(top = indicatorTop + 8.dp))
    }
}

/** The arrow, swept in from its tail as the pull grows, turning while it refreshes. */
@Composable private fun PullIndicator(progress: Float, refreshing: Boolean, modifier: Modifier) {
    val still = reduceMotion() || !refreshing
    val turn = if (still) 0f else rememberInfiniteTransition(label = "Refreshing")
        .animateFloat(0f, 360f, infiniteRepeatable(tween(800, easing = LinearEasing), RepeatMode.Restart), label = "Turn").value
    Box(modifier.padding(8.dp).graphicsLayer { alpha = if (refreshing) 1f else minOf(1f, progress * 2) }
        .then(if (refreshing) Modifier.clearAndSetSemantics { contentDescription = "Refreshing" } else Modifier.clearAndSetSemantics {})) {
        Symbol("arrow.clockwise", Modifier.rotate(turn).drawWithContent {
            val radius = hypot(size.width, size.height)
            val wedge = Path().apply {
                moveTo(center.x, center.y)
                arcTo(Rect(center - Offset(radius, radius), center + Offset(radius, radius)), SweepStart, 360f * progress, false)
                close()
            }
            clipPath(wedge) { this@drawWithContent.drawContent() }
        }, size = 22.dp, tint = Palette.secondary)
    }
}

/** Where the arrow's tail is: the Material arrow starts at 3 o'clock (SF's just above it, at -15°). */
private const val SweepStart = 0f
