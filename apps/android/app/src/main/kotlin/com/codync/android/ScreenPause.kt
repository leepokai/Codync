package com.codync.android

import android.os.SystemClock
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberUpdatedState
import kotlinx.coroutines.delay

/** A streaming screen pauses after this long without anyone touching the app (desktop and iPhone wait as long). */
internal const val SCREEN_PAUSE_AFTER_MS = 5 * 60_000L

/** The line on a screen picture's play button: before it first streams ([Start]) or once it paused ([Paused]). */
internal enum class ScreenPrompt(val text: String) { Start("Tap to see the screen"), Paused("Paused. Tap to continue.") }

/**
 * How long until a screen that started streaming at [start] pauses, counting from the later of that and the [lastTouch]
 * (all [SystemClock.elapsedRealtime]); zero or less: now.
 */
internal fun pauseIn(lastTouch: Long, start: Long, now: Long) = SCREEN_PAUSE_AFTER_MS - (now - maxOf(lastTouch, start))

/** When someone last touched the app: `MainActivity` sees every touch without taking it, the remote screen's keyboard every key. */
internal object AppTouches {
    @Volatile private var last = 0L

    fun touched() { last = SystemClock.elapsedRealtime() }

    /** Returns once nobody has touched the app for [SCREEN_PAUSE_AFTER_MS], counted from this call at the earliest. */
    suspend fun awaitIdle() {
        val start = SystemClock.elapsedRealtime()
        while (true) {
            val left = pauseIn(last, start, SystemClock.elapsedRealtime())
            if (left <= 0) return
            delay(left)
        }
    }
}

/**
 * While [session] streams, [pause] it once nobody has touched the app for a while, and report where ([screen]). Connecting,
 * reconnecting or failed doesn't count; streaming again starts the count over.
 */
@Composable internal fun PauseWhenIdle(session: ScreenSession?, screen: AnalyticsScreen, pause: () -> Unit) {
    val current by rememberUpdatedState(pause)
    val streaming = session?.phase == ScreenSession.Phase.Live
    LaunchedEffect(session, streaming) {
        if (!streaming) return@LaunchedEffect
        AppTouches.awaitIdle()
        Analytics.capture(AnalyticsEvent.RemoteScreenPaused(screen))
        current()
    }
}
