package com.codync.android

import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.drag
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.*
import androidx.compose.ui.unit.dp
import com.codync.android.core.ModelOption
import com.codync.android.design.*
import kotlin.math.roundToInt

/** Discrete reasoning levels from the harness; other model options retain their own controls. */
@Composable internal fun ReasoningSlider(option: ModelOption, value: String?, choose: (String?) -> Unit) {
    val index = option.values.indexOfFirst { it.id == (value ?: option.default) }
    val last = (option.values.size - 1).coerceAtLeast(1)
    val name = option.values.getOrNull(index)?.name.orEmpty()
    val chooseCurrent by rememberUpdatedState(choose)
    val position by animateFloatAsState(index.coerceAtLeast(0).toFloat() / last, Motion.hover(), label = "Reasoning position")
    val track = Palette.surface
    val ink = Palette.text
    val dots = Palette.secondary
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(option.name, style = Type.caption, color = Palette.secondary)
            Box(Modifier.weight(1f), contentAlignment = Alignment.Center) {
                Crossfade(name, animationSpec = Motion.fade(), label = "Reasoning level") { text ->
                    Text(text, style = Type.subheadline, color = Palette.text)
                }
            }
            LinkButton("Default", style = Type.caption) { choose(null) }
            Symbol(if (value == null) "checkmark" else "arrow.counterclockwise", size = 10.dp, tint = Palette.secondary)
        }
        Canvas(Modifier.fillMaxWidth().height(36.dp)
            .semantics {
                contentDescription = option.name
                stateDescription = name.ifEmpty { "Default" }
                progressBarRangeInfo = ProgressBarRangeInfo(index.coerceAtLeast(0).toFloat(), 0f..last.toFloat(), (last - 1).coerceAtLeast(0))
                setProgress { progress ->
                    option.values.getOrNull(progress.roundToInt().coerceIn(0, last))?.let { chooseCurrent(it.id) }
                    true
                }
            }
            .pointerInput(option.id, option.values) {
                fun pick(x: Float) {
                    val radius = 15.dp.toPx()
                    val travel = (size.width - radius * 2).coerceAtLeast(1f)
                    val stop = (((x - radius) / travel).coerceIn(0f, 1f) * last).roundToInt()
                    option.values.getOrNull(stop)?.let { chooseCurrent(it.id) }
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
            if (index >= 0) drawRoundRect(ink.copy(alpha = .14f), Offset(0f, center - radius), Size(left + radius * 2, radius * 2), CornerRadius(radius))
            option.values.indices.forEach { stop -> drawCircle(dots, 2.dp.toPx(), Offset(radius + stop.toFloat() / last * travel, center)) }
            if (index >= 0) drawCircle(ink, radius, Offset(radius + left, center))
        }
    }
}
