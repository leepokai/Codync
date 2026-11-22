package com.codync.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ConversationScrollDecisionTest {
    @Test fun `a loaded chat opens on its newest row`() {
        assertEquals(299, firstLayoutIndex(300))
        assertEquals(0, firstLayoutIndex(1))
    }

    @Test fun `no rows yet keeps the request past the end so the first real layout clamps to the newest`() {
        assertEquals(Int.MAX_VALUE, firstLayoutIndex(0))
        assertEquals(Int.MAX_VALUE, firstLayoutIndex(-1))
    }

    @Test fun `the reader at the end follows a new row`() {
        assertTrue(followNewest(following = true, touched = false))
        assertEquals(301, firstLayoutIndex(302))
    }

    @Test fun `a finger scroll or a reader who left the end stays put`() {
        assertFalse(followNewest(following = true, touched = true))
        assertFalse(followNewest(following = false, touched = false))
        assertFalse(followNewest(following = false, touched = true))
    }

    @Test fun `older rows prepended while the reader is away do not follow to a new index`() {
        assertFalse(followNewest(following = false, touched = false))
        assertTrue(followNewest(following = true, touched = false))
        assertEquals(379, firstLayoutIndex(380))
    }
}
