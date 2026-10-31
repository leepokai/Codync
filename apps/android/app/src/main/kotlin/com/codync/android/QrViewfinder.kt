package com.codync.android

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.SystemClock
import android.provider.Settings
import android.util.Size
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.*
import androidx.camera.core.resolutionselector.ResolutionSelector
import androidx.camera.core.resolutionselector.ResolutionStrategy
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Text
import androidx.compose.foundation.background
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextAlign
import com.codync.android.design.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.Observer
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.codync.android.core.PairingQr
import com.codync.android.core.PairingScan
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.ui.input.pointer.pointerInput
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.delay
import java.util.concurrent.atomic.AtomicBoolean

/**
 * The camera, right on the pairing page (iOS `QRScanner` in its viewfinder): asks for the camera
 * in place, then reads Codync pairing codes. Nothing is saved.
 */
@Composable fun QrViewfinder(found: (String) -> Unit, modifier: Modifier = Modifier, error: (String?) -> Unit = {}) {
    val context = LocalContext.current
    val hardware = remember { context.packageManager.hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY) }
    var asked by remember { mutableStateOf(false) }
    var granted by remember { mutableStateOf(false) }
    val permitted = rememberOnResume { ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED } || granted
    val permission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted = it; asked = true }
    Box(modifier.background(Palette.surface), contentAlignment = Alignment.Center) {
        if (hardware && permitted) CameraPairingPreview(Modifier.fillMaxSize(), found, error)
        else Column(Modifier.padding(24.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Symbol("qrcode.viewfinder", size = 52.dp, tint = Palette.tertiary)
            Text(if (!hardware) "Camera not available. Paste the link below." else "Allow the camera to scan the code on your computer.",
                style = Type.footnote, color = Palette.tertiary, textAlign = TextAlign.Center)
            if (hardware) {
                if (!asked) SecondaryButton("Allow camera") { permission.launch(Manifest.permission.CAMERA) }
                else SecondaryButton("Camera settings") {
                    context.startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:${context.packageName}")))
                }
            }
        }
    }
}

/** CameraX owns foreground lifecycle. Every analyzed frame closes; no image or pairing link is saved. */
@Composable private fun CameraPairingPreview(modifier: Modifier, found: (String) -> Unit, error: (String?) -> Unit) {
    val context = LocalContext.current
    val owner = LocalLifecycleOwner.current
    val onFound by rememberUpdatedState(found)
    val onError by rememberUpdatedState(error)
    val view = remember { PreviewView(context).apply { implementationMode = PreviewView.ImplementationMode.COMPATIBLE; scaleType = PreviewView.ScaleType.FILL_CENTER } }
    var camera by remember { mutableStateOf<Camera?>(null) }
    var torch by remember { mutableStateOf(false) }
    DisposableEffect(owner, view) {
        val closed = AtomicBoolean(false)
        val accepted = AtomicBoolean(false)
        val executor = Executors.newSingleThreadExecutor()
        val main = ContextCompat.getMainExecutor(context)
        val reader = PairingQr(BuildConfig.URL_SCHEME)
        val preview = Preview.Builder().build().apply { setSurfaceProvider(view.surfaceProvider) }
        // A pairing code is dense and small on a laptop screen: at 720p it reads only from so close that many
        // main cameras can't focus. 1080p reads it from farther back, where they can.
        val analysis = ImageAnalysis.Builder().setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
            .setResolutionSelector(ResolutionSelector.Builder().setResolutionStrategy(
                ResolutionStrategy(Size(1920, 1080), ResolutionStrategy.FALLBACK_RULE_CLOSEST_LOWER_THEN_HIGHER)).build()).build()
        var lastFrame = 0L
        analysis.setAnalyzer(executor) { image ->
            try {
                val now = SystemClock.elapsedRealtime()
                if (!closed.get() && !accepted.get() && now - lastFrame >= 150) {
                    lastFrame = now
                    val plane = image.planes.first()
                    val pixels = PairingQr.luminance(plane.buffer, image.width, image.height, plane.rowStride, plane.pixelStride)
                    val result = try { reader.decode(pixels, image.width, image.height) } finally { pixels.fill(0) }
                    if (result != null) main.execute {
                        if (!closed.get() && owner.lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) when (result) {
                            is PairingScan.Found -> if (accepted.compareAndSet(false, true)) onFound(result.link)
                            is PairingScan.Invalid -> onError(result.message)
                        }
                    }
                }
            } catch (_: Exception) { /* A damaged/partial camera frame can be skipped. */ }
            finally { image.close() }
        }
        var provider: ProcessCameraProvider? = null
        val cameraState = Observer<CameraState> { state ->
            if (!closed.get() && state.error != null) onError("Couldn't open the camera. Retry or use a pairing link.")
        }
        val future = ProcessCameraProvider.getInstance(context)
        future.addListener({
            if (!closed.get()) try {
                if (ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) != PackageManager.PERMISSION_GRANTED) {
                    onError("Allow camera access or use a pairing link.")
                    return@addListener
                }
                val resolved = future.get()
                provider = resolved
                val selector = if (resolved.hasCamera(CameraSelector.DEFAULT_BACK_CAMERA)) CameraSelector.DEFAULT_BACK_CAMERA else CameraSelector.DEFAULT_FRONT_CAMERA
                val bound = resolved.bindToLifecycle(owner, selector, preview, analysis)
                camera = bound
                bound.cameraInfo.cameraState.observe(owner, cameraState)
            } catch (_: Exception) { onError("Couldn't open the camera. Retry or use a pairing link.") }
        }, main)
        onDispose {
            closed.set(true); analysis.clearAnalyzer()
            camera?.cameraInfo?.cameraState?.removeObserver(cameraState)
            provider?.unbind(preview, analysis)
            camera = null
            executor.shutdownNow()
        }
    }
    // Continuous focus alone hunts on a glowing screen or settles on what's behind the code: focus where the code is
    // (the middle, or where it was tapped) again every couple of seconds until it reads.
    var focusAt by remember { mutableStateOf<MeteringPoint?>(null) }
    LaunchedEffect(camera, focusAt) {
        val active = camera ?: return@LaunchedEffect
        val point = focusAt ?: SurfaceOrientedMeteringPointFactory(1f, 1f).createPoint(0.5f, 0.5f, 0.3f)
        while (true) {
            active.cameraControl.startFocusAndMetering(FocusMeteringAction.Builder(point, FocusMeteringAction.FLAG_AF or FocusMeteringAction.FLAG_AE)
                .setAutoCancelDuration(2, TimeUnit.SECONDS).build())
            delay(2_500)
        }
    }
    Box(modifier) {
        AndroidView(factory = { view }, modifier = Modifier.fillMaxSize().pointerInput(Unit) {
            detectTapGestures { focusAt = view.meteringPointFactory.createPoint(it.x, it.y) }
        })
        if (camera?.cameraInfo?.hasFlashUnit() == true) Box(Modifier.align(Alignment.BottomEnd).padding(12.dp)) {
            IconButton(if (torch) "Turn flashlight off" else "Turn flashlight on", "bolt", tint = Color.White) {
                val active = camera ?: return@IconButton
                val next = !torch
                val result = active.cameraControl.enableTorch(next)
                result.addListener({
                    if (camera === active) try { result.get(); torch = next }
                    catch (_: Exception) { onError("The flashlight is unavailable. Try again.") }
                }, ContextCompat.getMainExecutor(context))
            }
        }
    }
}
