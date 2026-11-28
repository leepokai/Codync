package com.codync.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class SwipeToReplyTest {
    private val threshold = ReplySwipe.THRESHOLD_DP

    @Test fun `the bubble follows the finger up to the threshold`() {
        assertEquals(40f, ReplySwipe.offset(40f, threshold))
        assertEquals(threshold, ReplySwipe.offset(threshold, threshold))
    }

    @Test fun `past the threshold the bubble resists, a quarter of the extra pull`() {
        assertEquals(threshold + 4f, ReplySwipe.offset(threshold + 16f, threshold))
    }

    @Test fun `a drag the other way moves nothing`() {
        assertEquals(0f, ReplySwipe.offset(-80f, threshold))
        assertEquals(0f, ReplySwipe.offset(0f, threshold))
    }

    @Test fun `the bubble never moves past the open space beside it`() {
        assertEquals(30f, ReplySwipe.shown(80f, 30f))
        assertEquals(40f, ReplySwipe.shown(40f, 100f))
        assertEquals(0f, ReplySwipe.shown(80f, 0f))
        assertEquals(0f, ReplySwipe.shown(80f, -5f))
        assertTrue(ReplySwipe.offset(threshold * 2f, threshold) >= threshold)
    }

    @Test fun `the reply arrow is grown and opaque at the threshold`() {
        assertEquals(0f, ReplySwipe.progress(0f, threshold))
        assertEquals(0.5f, ReplySwipe.progress(threshold / 2f, threshold))
        assertEquals(1f, ReplySwipe.progress(threshold * 2f, threshold))
    }

    @Test fun `a swiped reply reports reply_swiped with no properties`() {
        assertEquals("reply_swiped", AnalyticsEvent.ReplySwiped.name)
        assertTrue(AnalyticsEvent.ReplySwiped.properties.isEmpty())
    }
}
