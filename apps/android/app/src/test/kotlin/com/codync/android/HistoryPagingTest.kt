package com.codync.android

import com.codync.android.core.Entry
import com.codync.android.core.WireJson
import kotlinx.serialization.encodeToString
import org.junit.Assert.assertEquals
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

    @Test fun `a page of only tool or thought rows still moves the window`() {
        // A full window of chat rows, and below it a page from the computer holding no chat row.
        val chat = (101L..400L).map { row(it, "user") }
        val page = (1L..100L).map { row(it, if (it % 2 == 0L) "tool" else "thought") }
        val stored = (page + chat).sortedByDescending { it.seq }.map { WireJson.encodeToString(it) }
        fun floor(residency: Residency) = newestWindow(stored.asSequence(), residency.limit("a"), Long.MAX_VALUE).minOf { it.seq }
        val open = Residency(setOf("a"))
        assertEquals(101L, floor(open))
        assertEquals(1L, floor(open.copy(windows = mapOf("a" to open.widened("a")))))
    }

    @Test fun `a window stops once its rows pass the byte budget, its newest row always kept`() {
        val stored = (10L downTo 1L).map { WireJson.encodeToString(row(it, "user")) }
        val threeRows = stored.take(3).sumOf { it.length }.toLong()
        assertEquals((8L..10L).toList(), newestWindow(stored.asSequence(), 10, threeRows).map { it.seq })
        assertEquals(listOf(10L), newestWindow(stored.asSequence(), 10, 1).map { it.seq })
        assertEquals((6L..10L).toList(), newestWindow(stored.asSequence(), 5, Long.MAX_VALUE).map { it.seq })
    }

    @Test fun `a page that failed is asked for again once the row shows anew, never in a loop`() {
        val paging = EarlierPaging()
        assertTrue(paging.ask(settled = true, visible = true, loading = false, shown = 10, floor = 40))
        assertFalse(paging.ask(settled = true, visible = true, loading = true, shown = 10, floor = 40))
        paging.failed()
        // The load ended with the row still up: no second request right away.
        assertFalse(paging.ask(settled = true, visible = true, loading = false, shown = 10, floor = 40))
        assertFalse(paging.ask(settled = true, visible = false, loading = false, shown = 10, floor = 40))
        assertTrue(paging.ask(settled = true, visible = true, loading = false, shown = 10, floor = 40))
        // A page that loaded but moved nothing isn't asked for again when the row comes back.
        assertFalse(paging.ask(settled = true, visible = false, loading = false, shown = 10, floor = 40))
        assertFalse(paging.ask(settled = true, visible = true, loading = false, shown = 10, floor = 40))
    }

    @Test fun `a failed page is asked for again after the list dropped its row unseen`() {
        val paging = EarlierPaging()
        assertTrue(paging.ask(settled = true, visible = true, loading = false, shown = 10, floor = 40))
        paging.failed()
        // Scrolled far away: the row leaves composition without ever asking while hidden.
        paging.left()
        assertTrue(paging.ask(settled = true, visible = true, loading = false, shown = 10, floor = 40))
        // Leaving after a page that loaded asks nothing more.
        paging.left()
        assertFalse(paging.ask(settled = true, visible = true, loading = false, shown = 10, floor = 40))
    }

    private fun row(seq: Long, kind: String) = Entry("e-$seq", seq, "a", rev = seq, kind = kind)
}
