package com.codync.android.core

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The voice pause and reading speed settings: a stored value snaps to the stops the slider offers. */
class VoiceSettingsTest {
    @Test fun thePauseSnapsToTheNearestStopAndStaysInRange() {
        assertEquals(1_500L, VoicePause.nearest(1_500L))
        assertEquals(1_500L, VoicePause.nearest(1_460L))
        assertEquals(600L, VoicePause.nearest(0L))
        assertEquals(3_000L, VoicePause.nearest(9_000L))
        assertTrue(VoicePause.nearest(999_999L) in VoicePause.MIN..VoicePause.MAX)
        assertEquals(25, VoicePause.stops.size)
        assertEquals(600L, VoicePause.stops.first())
        assertEquals(3_000L, VoicePause.stops.last())
    }

    @Test fun theReadingSpeedSnapsToTheNearestOfferedSpeed() {
        assertEquals(1f, VoiceRates.nearest(1f), 0f)
        assertEquals(.75f, VoiceRates.nearest(.84f), 0f)
        assertEquals(1f, VoiceRates.nearest(1.12f), 0f)
        assertEquals(.5f, VoiceRates.nearest(.1f), 0f)
        assertEquals(2f, VoiceRates.nearest(9f), 0f)
    }

    @Test fun settingsAcceptTheWholeSliderRangeAndRejectWhatItCannotReach() {
        assertEquals(700L, VoiceSettings(pauseMillis = 700).pauseMillis)
        assertTrue(runCatching { VoiceSettings(pauseMillis = 500) }.isFailure)
        assertTrue(runCatching { VoiceSettings(pauseMillis = 3_500) }.isFailure)
        assertTrue(runCatching { VoiceSettings(rate = 2.5f) }.isFailure)
    }
}
