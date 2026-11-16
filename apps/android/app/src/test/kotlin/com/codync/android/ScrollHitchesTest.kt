package com.codync.android

import kotlinx.serialization.json.JsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ScrollHitchesTest {
    @Test fun `steady sixty and hundred twenty fps frames do not hitch`() {
        for (fps in listOf(60, 120)) {
            val accumulator = ScrollHitchAccumulator()
            val expected = 1_000_000_000.0 / fps
            repeat(fps) { accumulator.add((1_000_000_000L / fps), expected) }
            val event = accumulator.event(1_000_000_000, ScrollList.Chat, 20, fps)!!
            assertEquals(fps, event.frames)
            assertEquals(0, event.hitches)
            assertEquals(0, event.hitchMsPerS)
        }
    }

    @Test fun `one fifty millisecond frame reports its hitch time`() {
        val accumulator = ScrollHitchAccumulator()
        val expected = 1_000_000_000.0 / 60
        repeat(59) { accumulator.add(16_666_667, expected) }
        accumulator.add(50_000_000, expected)
        val event = accumulator.event(1_000_000_000, ScrollList.Chat, 20, 60)!!
        assertEquals(60, event.frames)
        assertEquals(1, event.hitches)
        assertEquals(50, event.longestMs)
        assertEquals(33, event.hitchMsPerS)
    }

    @Test fun `exactly one and a half expected intervals is not a hitch`() {
        val accumulator = ScrollHitchAccumulator()
        accumulator.add(30_000_000, 20_000_000.0)
        assertEquals(0, accumulator.event(1_000_000_000, ScrollList.Bots, 20, 60)!!.hitches)
    }

    @Test fun `a window without drawn frames sends no event`() {
        assertNull(ScrollHitchAccumulator().event(300_000_000, ScrollList.Bots, 20, 60))
    }

    @Test fun `scroll event properties include chat stop and jump`() {
        val event = AnalyticsEvent.ScrollHitches(ScrollList.Chat, 42, 120, 2, 50, 33, 60, ScrollStop.End, 18)
        assertEquals("scroll_hitches", event.name)
        assertEquals("""{"list":"chat","rows":42,"frames":120,"hitches":2,"longest_ms":50,"hitch_ms_per_s":33,"fps":60,"stop":"end","jump":18}""",
            JsonObject(event.properties).toString())
    }

    @Test fun `bot scroll event omits chat only properties`() {
        val event = AnalyticsEvent.ScrollHitches(ScrollList.Bots, 8, 90, 0, 17, 0, 90)
        assertEquals("""{"list":"bots","rows":8,"frames":90,"hitches":0,"longest_ms":17,"hitch_ms_per_s":0,"fps":90}""",
            JsonObject(event.properties).toString())
    }
}
