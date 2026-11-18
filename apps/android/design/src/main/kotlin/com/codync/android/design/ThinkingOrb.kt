package com.codync.android.design

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableDoubleStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.withFrameNanos
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.repeatOnLifecycle

enum class ThinkingOrbState { Working, Searching, Listening, Connecting, Shaping }

/** The halftone thinking orb: what a bot is doing, as light flowing over a fixed dot grid; decorative only. */
@Composable fun ThinkingOrb(state: ThinkingOrbState = ThinkingOrbState.Working, size: Dp = 16.dp, color: Color = Palette.text,
    animated: Boolean = true, modifier: Modifier = Modifier) {
    val points = with(LocalDensity.current) { size.toPx() }.toDouble()
    var time by remember(state) { mutableDoubleStateOf(ThinkingOrbGeometry.rest(state)) }
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    if (animated && !reduceMotion()) LaunchedEffect(state, points) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.RESUMED) {
            val start = withFrameNanos { it }
            var last = 0L
            while (true) withFrameNanos { now ->
                // 30 fps, like the iOS TimelineView.
                if (now - last >= 33_000_000L) { last = now; time = ThinkingOrbGeometry.rest(state) + (now - start) / 1e9 }
            }
        }
    }
    val scale = points / size.value
    Canvas(modifier.size(size).clearAndSetSemantics {}) {
        for (dot in ThinkingOrbGeometry.frame(state, size.value.toDouble(), time)) drawCircle(color.copy(alpha = color.alpha * dot.alpha.toFloat()),
            (dot.radius * scale).toFloat(), Offset((dot.x * scale).toFloat(), (dot.y * scale).toFloat()))
    }
}

/** The system's "Remove animations" setting, iOS Reduce Motion's counterpart. */
@Composable fun reduceMotion(): Boolean {
    val context = androidx.compose.ui.platform.LocalContext.current
    return remember { android.provider.Settings.Global.getFloat(context.contentResolver, android.provider.Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f }
}
