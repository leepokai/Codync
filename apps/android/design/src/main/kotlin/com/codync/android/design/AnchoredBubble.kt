package com.codync.android.design

import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.Animatable
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.dismiss
import androidx.compose.ui.semantics.paneTitle
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.kyant.shapes.RoundedRectangle

/** A small panel above its anchor, dismissed outside or with Back, using the shared menu placement. */
@Composable fun AnchoredBubble(isPresented: Boolean, onDismiss: () -> Unit, title: String, anchor: @Composable () -> Unit,
    content: @Composable () -> Unit) {
    var bounds by remember { mutableStateOf(Rect.Zero) }
    Box(Modifier.onGloballyPositioned { bounds = it.boundsInRoot() }) { anchor() }
    val shown = remember { Animatable(0f) }
    var mounted by remember { mutableStateOf(false) }
    LaunchedEffect(isPresented) {
        if (isPresented) { mounted = true; shown.animateTo(1f, Motion.layout()) }
        else { shown.animateTo(0f, Motion.fade()); mounted = false }
    }
    if (!mounted) return
    BackHandler(isPresented, onDismiss)
    Overlay {
        BoxWithConstraints(Modifier.fillMaxSize()) {
            Box(Modifier.fillMaxSize().clickable(remember { MutableInteractionSource() }, null, onClick = onDismiss))
            val density = LocalDensity.current
            MenuPlacement(bounds, with(density) { maxWidth.toPx() }, with(density) { maxHeight.toPx() },
                cover = false, shown = shown.value, preferAbove = true) {
                val shape = RoundedRectangle(16.dp)
                Box(Modifier.width(250.dp).glass(shape, GlassStyle.Menu).clip(shape)
                    .pointerInput(Unit) { detectTapGestures(onTap = {}) }
                    .semantics { paneTitle = title; dismiss { onDismiss(); true } }
                    .padding(16.dp)) { content() }
            }
        }
    }
}
