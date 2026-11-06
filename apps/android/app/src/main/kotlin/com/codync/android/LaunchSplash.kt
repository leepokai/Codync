package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.EnterTransition
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleOut
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import kotlinx.coroutines.delay
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.codync.android.design.CharacterAvatar
import com.codync.android.design.Palette
import com.codync.android.design.reduceMotion
import kotlin.math.PI
import kotlin.math.sin

/**
 * Cold launch, as iOS `LaunchSplash`: the app-icon bot pops out of the blank system splash and
 * looks around while the computer catches up, then grows a little as it fades into the app.
 */
/**
 * The splash over the app until the account is open and its computers have caught up ([caughtUp]; true with none), held
 * at least 0.7 s so the motion reads and never past 1.8 s, the cached list being behind it (iOS `LaunchSplash`).
 */
@Composable internal fun LaunchLayer(loading: Boolean, caughtUp: Boolean) {
    var visible by remember { mutableStateOf(true) }
    val started = remember { System.currentTimeMillis() }
    LaunchedEffect(loading, caughtUp) {
        if (loading) return@LaunchedEffect
        delay(maxOf(0L, 700 - (System.currentTimeMillis() - started)))
        if (!caughtUp) delay(maxOf(0L, 1_800 - (System.currentTimeMillis() - started)))
        visible = false
    }
    AnimatedVisibility(visible, enter = EnterTransition.None,
        exit = fadeOut(tween(300)) + scaleOut(tween(300), targetScale = if (reduceMotion()) 1f else 1.15f)) { LaunchSplash() }
}

@Composable private fun LaunchSplash() {
    val still = reduceMotion()
    val shown = remember { Animatable(if (still) 1f else 0.6f) }
    val opacity = remember { Animatable(0f) }
    LaunchedEffect(Unit) {
        opacity.animateTo(1f, tween(200))
    }
    LaunchedEffect(Unit) { shown.animateTo(1f, spring(dampingRatio = 0.55f, stiffness = 158f)) }
    val bob by rememberInfiniteTransition(label = "Bob").animateFloat(0f, 1f,
        infiniteRepeatable(tween((2 * PI / 2.4 * 1000).toInt(), easing = LinearEasing), RepeatMode.Restart), label = "Phase")
    Box(Modifier.fillMaxSize().background(Palette.background).semantics { contentDescription = "Connecting" }, contentAlignment = Alignment.Center) {
        Box(Modifier.graphicsLayer {
            scaleX = shown.value; scaleY = shown.value; alpha = opacity.value
            translationY = if (still) 0f else sin(bob * 2 * PI).toFloat() * 4.dp.toPx()
        }) { CharacterAvatar("squircle", "gray", 112.dp, status = "working", monochrome = Palette.text) }
    }
}
