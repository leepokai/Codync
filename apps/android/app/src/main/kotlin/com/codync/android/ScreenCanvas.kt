package com.codync.android

import android.annotation.SuppressLint
import android.content.Context
import android.graphics.Matrix
import android.graphics.RectF
import android.graphics.SurfaceTexture
import android.view.TextureView
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.PointerInputScope
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.util.VelocityTracker
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import com.codync.android.core.*
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import org.webrtc.EglBase
import org.webrtc.EglRenderer
import org.webrtc.GlRectDrawer
import org.webrtc.VideoFrame
import org.webrtc.VideoSink
import kotlin.math.abs
import kotlin.math.hypot

/**
 * The video plus every touch and key gesture (CodyncUI `ScreenCanvas`). `ScreenControl` turns
 * them into input; coordinates leave here as view points (dp), the control maps them to the display.
 */
@Composable internal fun ScreenCanvas(session: ScreenSession, display: ScreenDisplay, mode: TouchMode, interactive: Boolean,
    keyboard: Boolean, armed: List<String>, onArmed: (List<String>) -> Unit, onZoomed: (Boolean) -> Unit, resetToken: Int,
    modifier: Modifier = Modifier) {
    val density = LocalDensity.current.density
    val haptics = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    val control = remember(session) { ScreenControl(session::send) }
    var frame by remember(session) { mutableStateOf(control.videoFrame) }
    var composing by remember { mutableStateOf(false) }
    val zoomed by rememberUpdatedState(onZoomed)
    val viewRequest = remember { object { var job: Job? = null } }
    SideEffect {
        control.mode = mode
        control.interactive = interactive
        control.armed = armed
        control.onArmedConsumed = { onArmed(emptyList()) }
        control.onChanged = { request ->
            frame = control.videoFrame
            zoomed(control.zoomed)
            // Streams just the visible region at the view's pixel size, once the gesture settles.
            if (request) {
                viewRequest.job?.cancel()
                viewRequest.job = scope.launch {
                    delay(180)
                    if (control.pinching) return@launch
                    val view = control.viewport.view
                    session.view(control.viewport.crop, ScreenSize(view.width * density, view.height * density))
                }
            }
        }
    }
    LaunchedEffect(control, display) { control.setDisplay(display) }
    LaunchedEffect(control, session.streamCrop) { control.setStreamRegion(session.streamCrop) }
    var handledReset by remember { mutableIntStateOf(resetToken) }
    LaunchedEffect(resetToken) { if (resetToken != handledReset) { handledReset = resetToken; control.resetZoom() } }

    Box(modifier.fillMaxSize().background(Color.Black).clipToBounds()
        .onSizeChanged { control.setViewSize(ScreenSize(it.width / density.toDouble(), it.height / density.toDouble())) }) {
        AndroidView({ context -> ScreenVideoView(context).also { session.sink = it } }, Modifier.fillMaxSize(),
            onRelease = { session.sink = null; it.release() }) { view ->
            view.place(frame.x * density, frame.y * density, frame.width * density, frame.height * density)
        }
        Box(Modifier.fillMaxSize().pointerInput(control) {
            screenGestures(control) { haptics.performHapticFeedback(HapticFeedbackType.LongPress) }
        })
        // Hidden while idle; while an IME composes, the field shows above the bottom edge so its marked text is visible.
        AndroidView({ context ->
            KeyProxyField(context).apply {
                onText = control::typed
                onKey = control::pressKey
                onComposingChanged = { composing = it }
                post { requestFocus() }
            }
        }, if (composing) Modifier.align(Alignment.BottomCenter).padding(16.dp).widthIn(min = 80.dp).heightIn(min = 40.dp)
            else Modifier.align(Alignment.TopStart).size(1.dp)) { field ->
            field.showsSoftKeyboard = keyboard
        }
    }
}

private enum class Gesture { Undecided, OnePan, Press, TwoFingers, Panning, Done }

/** UIKit's recognizers in one detector: tap, two-finger tap, pan, long-press drag, two-finger pan and pinch together. */
private suspend fun PointerInputScope.screenGestures(control: ScreenControl, haptic: () -> Unit) {
    val slop = viewConfiguration.touchSlop
    fun point(o: Offset) = ScreenPoint(o.x / density.toDouble(), o.y / density.toDouble())
    awaitEachGesture {
        val first = awaitFirstDown(requireUnconsumed = false)
        val velocity = VelocityTracker().apply { addPosition(first.uptimeMillis, first.position) }
        var gesture = Gesture.Undecided
        var last = first.position
        var pair: List<PointerInputChange> = emptyList()
        var startCenter = Offset.Zero
        var startSpread = 0f
        var center = Offset.Zero
        var spread = 0f
        var pinching = false
        try {
            while (true) {
                val wait = PRESS_MILLIS - (currentEvent.changes.first().uptimeMillis - first.uptimeMillis)
                val event = if (gesture == Gesture.Undecided) withTimeoutOrNull(maxOf(wait, 0)) { awaitPointerEvent() } else awaitPointerEvent()
                if (event == null) {
                    gesture = Gesture.Press
                    if (control.pressBegan(point(last))) haptic()
                    continue
                }
                event.changes.forEach { it.consume() }
                val down = event.changes.filter { it.pressed }
                val finger = event.changes.firstOrNull { it.id == first.id }
                when (gesture) {
                    Gesture.Undecided -> when {
                        down.size >= 2 -> {
                            gesture = Gesture.TwoFingers
                            pair = down.take(2)
                            startCenter = (pair[0].position + pair[1].position) / 2f
                            startSpread = (pair[0].position - pair[1].position).getDistance()
                            center = startCenter
                            spread = startSpread
                        }
                        finger == null || !finger.pressed -> {
                            control.tap(point(finger?.position ?: last), finger?.uptimeMillis ?: first.uptimeMillis)
                            return@awaitEachGesture
                        }
                        (finger.position - first.position).getDistance() > slop -> {
                            gesture = Gesture.OnePan
                            last = finger.position
                            velocity.addPosition(finger.uptimeMillis, finger.position)
                        }
                    }
                    Gesture.OnePan -> {
                        if (finger == null || !finger.pressed) return@awaitEachGesture
                        velocity.addPosition(finger.uptimeMillis, finger.position)
                        val v = velocity.calculateVelocity()
                        control.onePan(point(finger.position - last), hypot(v.x, v.y) / density.toDouble())
                        last = finger.position
                    }
                    Gesture.Press -> {
                        if (finger == null || !finger.pressed) return@awaitEachGesture
                        control.pressMoved(point(finger.position))
                        last = finger.position
                    }
                    Gesture.TwoFingers, Gesture.Panning -> {
                        val now = pair.map { p -> event.changes.firstOrNull { it.id == p.id } }
                        if (now.any { it == null || !it.pressed }) {
                            val quick = event.changes.first().uptimeMillis - first.uptimeMillis < TWO_TAP_MILLIS
                            if (gesture == Gesture.TwoFingers && quick) control.twoFingerTap(point(startCenter))
                            if (pinching) { pinching = false; control.pinchEnded() }
                            gesture = Gesture.Done
                        } else {
                            val a = requireNotNull(now[0]).position
                            val b = requireNotNull(now[1]).position
                            val nextCenter = (a + b) / 2f
                            val nextSpread = (a - b).getDistance()
                            if (gesture == Gesture.TwoFingers && ((nextCenter - startCenter).getDistance() > slop || abs(nextSpread - startSpread) > slop))
                                gesture = Gesture.Panning
                            if (gesture == Gesture.Panning) {
                                if (!pinching && abs(nextSpread - startSpread) > slop) { pinching = true; control.pinchBegan() }
                                if (pinching && spread > 0f) control.pinch((nextSpread / spread).toDouble(), point(nextCenter))
                                control.twoPan(point(nextCenter - center), point(nextCenter))
                                center = nextCenter
                                spread = nextSpread
                            }
                        }
                    }
                    Gesture.Done -> if (down.isEmpty()) return@awaitEachGesture
                }
            }
        } finally {
            if (gesture == Gesture.Press) control.pressEnded()
            if (pinching) control.pinchEnded()
        }
    }
}

private const val PRESS_MILLIS = 350L
private const val TWO_TAP_MILLIS = 500L

/** WebRTC video in a TextureView: a normal view, so it moves, clips and sits under Compose chrome. */
@SuppressLint("ViewConstructor")
internal class ScreenVideoView(context: Context) : TextureView(context), TextureView.SurfaceTextureListener, VideoSink {
    private val renderer = EglRenderer("screen")

    init {
        renderer.init(ScreenRtc.egl.eglBaseContext, EglBase.CONFIG_PLAIN, GlRectDrawer())
        surfaceTextureListener = this
    }

    override fun onFrame(frame: VideoFrame) = renderer.onFrame(frame)

    private val placement = RectF()

    /** Shows the whole stream at this rectangle (pixels in the view); what falls outside is clipped. */
    fun place(left: Double, top: Double, width: Double, height: Double) {
        placement.set(left.toFloat(), top.toFloat(), (left + width).toFloat(), (top + height).toFloat())
        applyPlacement()
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        applyPlacement()
    }

    private fun applyPlacement() {
        if (width == 0 || height == 0 || placement.isEmpty) return
        setTransform(Matrix().apply { setRectToRect(RectF(0f, 0f, width.toFloat(), height.toFloat()), placement, Matrix.ScaleToFit.FILL) })
        invalidate()
    }

    fun release() = renderer.release()

    override fun onSurfaceTextureAvailable(surface: SurfaceTexture, width: Int, height: Int) = renderer.createEglSurface(surface)
    override fun onSurfaceTextureSizeChanged(surface: SurfaceTexture, width: Int, height: Int) = Unit
    // The EGL surface goes first; the texture is released once the render thread lets go of it.
    override fun onSurfaceTextureDestroyed(surface: SurfaceTexture): Boolean {
        renderer.releaseEglSurface { surface.release() }
        return false
    }
    override fun onSurfaceTextureUpdated(surface: SurfaceTexture) = Unit
}
