package com.codync.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ScrollHitchesTest {
    @Test fun `steady sixty and hundred twenty fps frames do not hitch`() {
        for (fps in listOf(60, 120)) {
            val accumulator = ScrollHitchAccumulator()
            val expected = 1_000_000_000.0 / fps
            repeat(fps) { accumulator.add((1_000_000_000L / fps), expected) }
            val event = accumulator.event(1_000_000_000, ScrollList.Chat, 20)!!
            assertEquals(fps, event.frames)
            assertEquals(0, event.hitches)
            assertEquals(0, event.hitchMsPerS)
            assertEquals(fps, event.fps)
        }
    }

    @Test fun `one fifty millisecond frame reports its hitch time`() {
        val accumulator = ScrollHitchAccumulator()
        val expected = 1_000_000_000.0 / 60
        repeat(59) { accumulator.add(16_666_667, expected) }
        accumulator.add(50_000_000, expected)
        val event = accumulator.event(1_000_000_000, ScrollList.Chat, 20)!!
        assertEquals(60, event.frames)
        assertEquals(1, event.hitches)
        assertEquals(50, event.longestMs)
        assertEquals(33, event.hitchMsPerS)
    }

    @Test fun `exactly one and a half expected intervals is not a hitch`() {
        val accumulator = ScrollHitchAccumulator()
        accumulator.add(30_000_000, 20_000_000.0)
        assertEquals(0, accumulator.event(1_000_000_000, ScrollList.Bots, 20)!!.hitches)
    }

    @Test fun `the top refresh rate is the shortest interval the display ran at`() {
        // Adaptive refresh drops to 60 while the scroll slows down; the display still runs up to 120.
        val accumulator = ScrollHitchAccumulator()
        accumulator.add(8_333_333, 1_000_000_000.0 / 120)
        accumulator.add(16_666_667, 1_000_000_000.0 / 60)
        assertEquals(120, accumulator.event(1_000_000_000, ScrollList.Bots, 20)!!.fps)
    }

    @Test fun `a window without drawn frames sends no event`() {
        assertNull(ScrollHitchAccumulator().event(300_000_000, ScrollList.Bots, 20))
    }

    @Test fun `scroll diagnostic includes chat stop and jump`() {
        val scroll = PhonePerformance.Scroll(ScrollList.Chat, 42, 120, 2, 50, 33, 60, ScrollStop.End, 18)
        assertEquals("""{"type":"scroll","platform":"android","list":"chat","rows":42,"frames":120,"hitches":2,"longestMs":50,"hitchMsPerS":33,"fps":60,"stop":"end","jump":18}""",
            scroll.body().toString())
    }

    @Test fun `bot scroll diagnostic omits chat only properties`() {
        val scroll = PhonePerformance.Scroll(ScrollList.Bots, 8, 90, 0, 17, 0, 90)
        assertEquals("""{"type":"scroll","platform":"android","list":"bots","rows":8,"frames":90,"hitches":0,"longestMs":17,"hitchMsPerS":0,"fps":90}""",
            scroll.body().toString())
    }
}
