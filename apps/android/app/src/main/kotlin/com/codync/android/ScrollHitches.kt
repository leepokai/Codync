package com.codync.android

import android.os.Build
import android.view.Choreographer
import androidx.compose.foundation.interaction.DragInteraction
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalView
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min
import kotlin.math.roundToInt
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull

/** Vsync intervals for one scroll, independent of Android so the measurement itself is unit-testable. */
internal class ScrollHitchAccumulator {
    private var frames = 0
    private var hitches = 0
    private var longestNanos = 0L
    private var hitchExcessNanos = 0.0
    /** The shortest frame interval the display ran at: its top rate (adaptive refresh slows down when it can). */
    private var shortestExpectedNanos = Double.POSITIVE_INFINITY

    fun add(intervalNanos: Long, expectedNanos: Double) {
        frames++
        longestNanos = max(longestNanos, intervalNanos)
        shortestExpectedNanos = min(shortestExpectedNanos, expectedNanos)
        if (intervalNanos > expectedNanos * 1.5) {
            hitches++
            hitchExcessNanos += intervalNanos - expectedNanos
        }
    }

    fun event(durationNanos: Long, list: ScrollList, rows: Int, stop: ScrollStop? = null, jump: Int? = null): AnalyticsEvent.ScrollHitches? {
        if (frames == 0) return null
        val hitchMsPerS = (hitchExcessNanos / 1_000_000.0 / (durationNanos / 1_000_000_000.0)).roundToInt()
        return AnalyticsEvent.ScrollHitches(list, rows, frames, hitches, (longestNanos / 1_000_000.0).roundToInt(), hitchMsPerS,
            (1_000_000_000.0 / shortestExpectedNanos).roundToInt(), stop, jump)
    }
}

/** Measures only reader drags; another drag during the settling tail continues the same event. */
@Composable internal fun TrackScrollHitches(list: LazyListState, kind: ScrollList) {
    // Emulator vsync says nothing about a physical phone's scrolling.
    if (Build.HARDWARE in listOf("ranchu", "goldfish")) return
    val view = LocalView.current
    val density = LocalDensity.current
    LaunchedEffect(list, kind) {
        val tailRestarts = Channel<Unit>(Channel.CONFLATED)
        var window: Job? = null
        var tail = false
        list.interactionSource.interactions.collect { interaction ->
            if (interaction !is DragInteraction.Start) return@collect
            if (tail) tailRestarts.trySend(Unit)
            tail = false
            if (window?.isActive == true) return@collect
            window = launch {
                val accumulator = ScrollHitchAccumulator()
                val startedAt = System.nanoTime()
                val choreographer = Choreographer.getInstance()
                var previousFrame: Long? = null
                var anchorKey: Any? = null
                var anchorOffset = 0
                var jump = 0
                var stop: ScrollStop? = null
                var running = true
                val callback = object : Choreographer.FrameCallback {
                    override fun doFrame(frameTimeNanos: Long) {
                        if (!running) return
                        // The rate this frame was asked to run at: adaptive refresh changes it mid-scroll.
                        val expectedNanos = 1_000_000_000.0 / view.display.refreshRate
                        previousFrame?.let { accumulator.add(frameTimeNanos - it, expectedNanos) }
                        previousFrame = frameTimeNanos
                        anchorKey?.let { key ->
                            list.layoutInfo.visibleItemsInfo.firstOrNull { it.key == key }?.let {
                                jump = max(jump, abs(it.offset - anchorOffset))
                            }
                        }
                        choreographer.postFrameCallback(this)
                    }
                }
                choreographer.postFrameCallback(callback)
                try {
                    while (true) {
                        // The drag's start lands a frame before the list reports scrolling.
                        withTimeoutOrNull(100) { snapshotFlow { list.isScrollInProgress }.first { it } }
                        snapshotFlow { list.isScrollInProgress }.first { !it }
                        tail = true
                        if (kind == ScrollList.Chat) {
                            val anchor = list.layoutInfo.visibleItemsInfo.firstOrNull()
                            anchorKey = anchor?.key
                            anchorOffset = anchor?.offset ?: 0
                            jump = 0
                            stop = when {
                                !list.canScrollForward -> ScrollStop.End
                                !list.canScrollBackward -> ScrollStop.Top
                                else -> ScrollStop.Middle
                            }
                        }
                        val restarted = withTimeoutOrNull(300) { tailRestarts.receive() } != null
                        tail = false
                        if (!restarted) break
                    }
                } finally {
                    running = false
                    choreographer.removeFrameCallback(callback)
                }
                val jumpDp = if (kind == ScrollList.Chat) with(density) { jump.toDp().value.roundToInt() } else null
                accumulator.event(System.nanoTime() - startedAt, kind, list.layoutInfo.totalItemsCount, stop, jumpDp)
                    ?.let(Analytics::capture)
            }
        }
    }
}
