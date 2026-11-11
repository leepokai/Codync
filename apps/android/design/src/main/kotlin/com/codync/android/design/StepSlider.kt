package com.codync.android.design

import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.drag
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.ProgressBarRangeInfo
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.progressBarRangeInfo
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.setProgress
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt

/**
 * A value picked by sliding along a track: a dot marks every stop, the track fills up to the choice
 * and the knob sits on it. The label sits above with the chosen value reading out beside it, and
 * TalkBack reads both and can swipe through the stops. Used where a choice would be too coarse (the
 * voice pause).
 */
@Composable fun <T> StepSlider(
    label: String,
    selection: T,
    stops: List<Pair<T, String>>,
    select: (T) -> Unit,
    modifier: Modifier = Modifier,
    detail: String? = null,
    enabled: Boolean = true,
) {
    if (stops.isEmpty()) return
    val index = stops.indexOfFirst { it.first == selection }.coerceAtLeast(0)
    val last = (stops.size - 1).coerceAtLeast(1)
    val readout = stops.getOrNull(index)?.second.orEmpty()
    val haptics = LocalCodyncHaptics.current
    val choose by rememberUpdatedState(select)
    val position by animateFloatAsState(index.toFloat() / last, Motion.hover(), label = "$label position")
    val track = Palette.surface
    val ink = Palette.text
    val dot = Palette.secondary
    Column(modifier.fillMaxWidth().graphicsLayer { alpha = if (enabled) 1f else .45f }, verticalArrangement = Arrangement.spacedBy(3.dp)) {
        Row(Modifier.padding(bottom = 2.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
                Text(label, style = Type.body, color = Palette.text)
                if (detail != null) Text(detail, style = Type.caption.copy(lineHeight = Type.caption.lineHeight * 1.12f), color = Palette.secondary)
            }
            Spacer(Modifier.width(12.dp))
            Crossfade(readout, animationSpec = Motion.fade(), label = "$label value") { text ->
                Text(text, style = Type.body, color = Palette.secondary)
            }
        }
        Canvas(Modifier.fillMaxWidth().height(36.dp).semantics {
            contentDescription = label
            stateDescription = readout
            progressBarRangeInfo = ProgressBarRangeInfo(index.toFloat(), 0f..last.toFloat(), (last - 1).coerceAtLeast(0))
            if (enabled) setProgress { progress ->
                stops.getOrNull(progress.roundToInt().coerceIn(0, last))?.let { choose(it.first) }
                true
            }
        }.pointerInput(stops, enabled) {
            if (!enabled) return@pointerInput
            fun pick(x: Float) {
                val radius = 15.dp.toPx()
                val travel = (size.width - radius * 2).coerceAtLeast(1f)
                val stop = (((x - radius) / travel).coerceIn(0f, 1f) * last).roundToInt()
                stops.getOrNull(stop)?.let {
                    if (it.first != selection) haptics.perform(CodyncHaptic.DragStep)
                    choose(it.first)
                }
            }
            awaitEachGesture {
                val down = awaitFirstDown()
                pick(down.position.x)
                drag(down.id) { change -> pick(change.position.x); change.consume() }
            }
        }) {
            val radius = 15.dp.toPx()
            val travel = (size.width - radius * 2).coerceAtLeast(0f)
            val center = size.height / 2
            val left = position * travel
            drawRoundRect(track, Offset(0f, center - radius), Size(size.width, radius * 2), CornerRadius(radius))
            drawRoundRect(ink.copy(alpha = .14f), Offset(0f, center - radius), Size(left + radius * 2, radius * 2), CornerRadius(radius))
            stops.indices.forEach { stop -> drawCircle(dot, 2.dp.toPx(), Offset(radius + stop.toFloat() / last * travel, center)) }
            drawCircle(ink, radius, Offset(radius + left, center))
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            Text(stops.first().second, style = Type.caption, color = Palette.tertiary)
            if (stops.size > 2) Text(stops.last().second, style = Type.caption, color = Palette.tertiary)
        }
    }
}
