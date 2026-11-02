package com.codync.android.design

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.windowInsetsTopHeight
import androidx.compose.foundation.layout.Spacer
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.nestedscroll.NestedScrollConnection
import androidx.compose.ui.input.nestedscroll.NestedScrollSource
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.Velocity
import androidx.compose.ui.unit.dp
import com.kyant.shapes.Capsule
import com.kyant.shapes.UnevenRoundedRectangle
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

/** Closes the modal the content is in (`\.dismissModal`). */
val LocalDismissModal = staticCompositionLocalOf<() -> Unit> { {} }

/**
 * The iOS sheet at its large detent: slides up over a dimmed app, a grabber on top, and
 * drags down to close (also from content scrolled to its top). Content titles itself with
 * `ModalHeader` and closes with `LocalDismissModal`.
 */
@Composable fun CodyncSheet(isPresented: Boolean, onDismiss: () -> Unit, fitted: Boolean = false, content: @Composable () -> Unit) {
    val shown = remember { Animatable(0f) }
    var mounted by remember { mutableStateOf(isPresented) }
    LaunchedEffect(isPresented) {
        if (isPresented) {
            mounted = true
            shown.animateTo(1f, Motion.present())
        } else {
            shown.animateTo(0f, Motion.present())
            mounted = false
        }
    }
    if (!mounted) return
    val dismiss by rememberUpdatedState(onDismiss)
    BackHandler(isPresented) { dismiss() }
    Overlay { SheetFrame(shown.value, fitted, { dismiss() }, content) }
}

/** `.codyncSheet(item:)`: keeps showing the last item while the sheet slides away. */
@Composable fun <T : Any> CodyncSheet(item: T?, onDismiss: () -> Unit, content: @Composable (T) -> Unit) {
    var last by remember { mutableStateOf(item) }
    if (item != null) last = item
    CodyncSheet(item != null, onDismiss) { last?.let { content(it) } }
}

@Composable private fun SheetFrame(shown: Float, fitted: Boolean, dismiss: () -> Unit, content: @Composable () -> Unit) {
    val dark = LocalDark.current
    val scope = rememberCoroutineScope()
    val drag = remember { Animatable(0f) }
    BoxWithConstraints(Modifier.fillMaxSize()) {
        val height = with(LocalDensity.current) { maxHeight.toPx() }
        val closeAt = height * 0.25f
        fun release(velocity: Float) {
            scope.launch {
                if (drag.value > closeAt || velocity > 1600f) dismiss() else drag.animateTo(0f, Motion.layout())
            }
        }
        val pull = remember {
            object : NestedScrollConnection {
                override fun onPreScroll(available: Offset, source: NestedScrollSource): Offset {
                    if (drag.value <= 0f || available.y >= 0f) return Offset.Zero
                    val used = maxOf(available.y, -drag.value)
                    scope.launch { drag.snapTo(drag.value + used) }
                    return Offset(0f, used)
                }

                override fun onPostScroll(consumed: Offset, available: Offset, source: NestedScrollSource): Offset {
                    if (available.y <= 0f || source != NestedScrollSource.UserInput) return Offset.Zero
                    scope.launch { drag.snapTo(drag.value + available.y) }
                    return Offset(0f, available.y)
                }

                override suspend fun onPreFling(available: Velocity): Velocity {
                    if (drag.value <= 0f) return Velocity.Zero
                    release(available.y)
                    return available
                }
            }
        }
        // The dimmed app behind; a tap outside closes, like the system sheet's.
        Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = (if (dark) 0.48f else 0.2f) * shown))
            .clickable(remember { MutableInteractionSource() }, null, onClick = dismiss))
        val offset = (1f - shown) * height + drag.value
        Column(Modifier.fillMaxSize(), verticalArrangement = if (fitted) Arrangement.Bottom else Arrangement.Top) {
            if (!fitted) {
                Spacer(Modifier.windowInsetsTopHeight(WindowInsets.statusBars))
                Spacer(Modifier.height(10.dp))
            }
            Box(Modifier.fillMaxWidth()
                .then(if (fitted) Modifier else Modifier.fillMaxSize())
                .offset { IntOffset(0, offset.roundToInt()) }
                .graphicsLayer { shadowElevation = if (dark) 0f else 24f; shape = SheetShape; clip = true }
                .background(Palette.background)
                .clickable(remember { MutableInteractionSource() }, null) {}
                .draggable(rememberDraggableState { delta -> scope.launch { drag.snapTo((drag.value + delta).coerceAtLeast(0f)) } },
                    Orientation.Vertical, onDragStopped = { velocity -> release(velocity) })
                .nestedScroll(pull)) {
                CompositionLocalProvider(LocalDismissModal provides dismiss) {
                    if (fitted) Column(Modifier.fillMaxWidth().navigationBarsPadding().imePadding()) { content() }
                    else GlassStage(Modifier.fillMaxSize().navigationBarsPadding().imePadding()) { content() }
                }
                Box(Modifier.fillMaxWidth().padding(top = 5.dp), contentAlignment = Alignment.TopCenter) {
                    Box(Modifier.size(36.dp, 5.dp).clip(Capsule()).background(if (dark) Color(0xFF5A5A5A) else Color(0xFFC6C6C6)))
                }
            }
        }
    }
}

private val SheetShape = UnevenRoundedRectangle(topStart = 40.dp, topEnd = 40.dp)
