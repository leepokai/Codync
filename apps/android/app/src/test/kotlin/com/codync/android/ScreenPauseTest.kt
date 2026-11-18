package com.codync.android

import kotlinx.serialization.json.JsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

class ScreenPauseTest {
    private val minute = 60_000L

    @Test fun `a screen pauses five minutes after the last touch, never before it started streaming`() {
        // Touched before streaming: the count starts with the stream.
        assertEquals(5 * minute, pauseIn(lastTouch = 0, start = 10 * minute, now = 10 * minute))
        assertEquals(minute, pauseIn(lastTouch = 0, start = 10 * minute, now = 14 * minute))
        assertEquals(0, pauseIn(lastTouch = 0, start = 10 * minute, now = 15 * minute))
        // A touch while streaming starts the count over.
        assertEquals(4 * minute, pauseIn(lastTouch = 13 * minute, start = 10 * minute, now = 14 * minute))
        assertEquals(-minute, pauseIn(lastTouch = 13 * minute, start = 10 * minute, now = 19 * minute))
    }

    @Test fun `the start and paused buttons say what a tap does, and a pause reports which picture`() {
        assertEquals("Tap to see the screen", ScreenPrompt.Start.text)
        assertEquals("Paused. Tap to continue.", ScreenPrompt.Paused.text)
        assertEquals("remote_screen_paused", AnalyticsEvent.RemoteScreenPaused(AnalyticsScreen.BotComputer).name)
        assertEquals("""{"screen":"bot_computer"}""", JsonObject(AnalyticsEvent.RemoteScreenPaused(AnalyticsScreen.BotComputer).properties).toString())
        assertEquals("""{"screen":"remote_screen"}""", JsonObject(AnalyticsEvent.RemoteScreenPaused(AnalyticsScreen.RemoteScreen).properties).toString())
    }
}
