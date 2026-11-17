package com.codync.android

import androidx.compose.animation.core.animate
import androidx.compose.foundation.Image
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculateCentroid
import androidx.compose.foundation.gestures.calculateCentroidSize
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.geometry.isSpecified
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerInputScope
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChanged
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.toSize
import com.codync.android.design.Motion
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlin.math.abs

/**
 * A picture that zooms ([ImageZoom]): pinch about the fingers, drag to move it once zoomed in, double-tap in at the tap and
 * again back to fit. At fit size a one-finger drag is left to the sheet, so it still swipes down to close; zoomed in, a
 * drag moves the picture instead. Each time `open` turns true it starts at fit. `zoomedIn` runs as it leaves fit size.
 */
@Composable internal fun ZoomableImage(bitmap: ImageBitmap, open: Boolean, modifier: Modifier = Modifier, zoomedIn: () -> Unit) {
    val inset = with(LocalDensity.current) { INSET.toPx() }
    var box by remember { mutableStateOf(Size.Zero) }
    var zoom by remember { mutableStateOf(ImageZoom.Fit) }
    var animation by remember { mutableStateOf<Job?>(null) }
    val scope = rememberCoroutineScope()
    val report by rememberUpdatedState(zoomedIn)
    LaunchedEffect(open) { if (open) { animation?.cancel(); zoom = ImageZoom.Fit } }
    // The picture's size at fit: inside the box, less the margin around it.
    fun picture() = ImageZoom.fitted(Size(bitmap.width.toFloat(), bitmap.height.toFloat()),
        Size((box.width - 2 * inset).coerceAtLeast(0f), (box.height - 2 * inset).coerceAtLeast(0f)))
    fun show(next: ImageZoom) {
        if (next.zoomedIn && !zoom.zoomedIn) report()
        zoom = next
    }
    // A new size (rotation, split screen) keeps the picture inside it, with no gap at an edge.
    Box(modifier.onSizeChanged { box = it.toSize(); zoom = zoom.clamped(box, picture()) }.clipToBounds()
        .pointerInput(bitmap) {
            detectTapGestures(onDoubleTap = { point ->
                val from = zoom
                val to = from.toggled(point - center(), box, picture())
                animation?.cancel()
                animation = scope.launch { animate(0f, 1f, animationSpec = Motion.layout()) { step, _ -> show(from.lerp(to, step).clamped(box, picture())) } }
            })
        }
        // Last, so it sees each move before the double tap, the menu's long press and the sheet's drag.
        .pointerInput(bitmap) {
            pinchAndPan({ zoom.zoomedIn }) { centroid, factor, pan ->
                animation?.cancel()
                show(zoom.zoomedBy(factor, centroid - center(), box, picture()).pannedBy(pan, box, picture()))
            }
        }) {
        Image(bitmap, contentDescription = null, contentScale = ContentScale.Fit, modifier = Modifier.fillMaxSize().graphicsLayer {
            scaleX = zoom.scale
            scaleY = zoom.scale
            translationX = zoom.offset.x
            translationY = zoom.offset.y
        }.padding(INSET))
    }
}

private val INSET = 12.dp

private fun PointerInputScope.center() = Offset(size.width / 2f, size.height / 2f)

/**
 * Two fingers, or one once `zoomed`, past the touch slop: each step's centroid, zoom factor and pan go to `change`, and the
 * moves are consumed so nothing behind takes them. One finger at fit size is left alone.
 */
private suspend fun PointerInputScope.pinchAndPan(zoomed: () -> Boolean, change: (Offset, Float, Offset) -> Unit) = awaitEachGesture {
    awaitFirstDown(requireUnconsumed = false)
    var factor = 1f
    var pan = Offset.Zero
    var moving = false
    do {
        val event = awaitPointerEvent()
        // The menu took the press (a long press), or the gesture ended elsewhere.
        if (event.changes.any { it.isConsumed }) break
        if (event.changes.count { it.pressed } < 2 && !zoomed()) continue
        val stepFactor = event.calculateZoom()
        val stepPan = event.calculatePan()
        if (!moving) {
            factor *= stepFactor
            pan += stepPan
            val slop = viewConfiguration.touchSlop
            moving = abs(1 - factor) * event.calculateCentroidSize(useCurrent = false) > slop || pan.getDistance() > slop
        }
        // The last finger lifting has no centroid left: nothing to move.
        val centroid = event.calculateCentroid(useCurrent = false)
        if (moving && centroid.isSpecified) {
            change(centroid, stepFactor, stepPan)
            event.changes.forEach { if (it.positionChanged()) it.consume() }
        }
    } while (event.changes.any { it.pressed })
}
