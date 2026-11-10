package com.codync.android

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class HistoryPagingTest {
    @Test fun `paging waits until the earlier row is on screen and a page would move the chat`() {
        assertFalse(pageEarlier(settled = false, visible = true, loading = false, shown = 10, floor = 40, askedShown = -1, askedFloor = Long.MAX_VALUE))
        assertFalse(pageEarlier(settled = true, visible = false, loading = false, shown = 10, floor = 40, askedShown = -1, askedFloor = Long.MAX_VALUE))
        assertFalse(pageEarlier(settled = true, visible = true, loading = true, shown = 10, floor = 40, askedShown = -1, askedFloor = Long.MAX_VALUE))
        assertTrue(pageEarlier(settled = true, visible = true, loading = false, shown = 10, floor = 40, askedShown = -1, askedFloor = Long.MAX_VALUE))
        assertFalse(pageEarlier(settled = true, visible = true, loading = false, shown = 10, floor = 40, askedShown = 10, askedFloor = 40))
        assertTrue(pageEarlier(settled = true, visible = true, loading = false, shown = 10, floor = 20, askedShown = 10, askedFloor = 40))
        assertTrue(pageEarlier(settled = true, visible = true, loading = false, shown = 12, floor = 40, askedShown = 10, askedFloor = 40))
    }
}
