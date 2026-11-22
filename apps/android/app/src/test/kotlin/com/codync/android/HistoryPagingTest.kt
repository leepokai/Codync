package com.codync.android

import com.codync.android.core.Entry
import com.codync.android.core.WireJson
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
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
        val chatStart = Residency.PAGE + 1L
        val chat = (chatStart until chatStart + Residency.DEFAULT_WINDOW).map { row(it, "user") }
        val page = (1L..Residency.PAGE.toLong()).map { row(it, if (it % 2 == 0L) "tool" else "thought") }
        val stored = (page + chat).sortedByDescending { it.seq }.map { WireJson.encodeToString(it) }
        fun floor(residency: Residency) = newestWindow(stored.asSequence(), residency.limit("a"), Long.MAX_VALUE).minOf { it.seq }
        val open = Residency(setOf("a"))
        assertEquals(chatStart, floor(open))
        assertEquals(1L, floor(open.copy(windows = mapOf("a" to open.widened("a")))))
    }

    @Test fun `a window stops once its rows pass the byte budget, its newest row always kept`() {
        val stored = (10L downTo 1L).map { WireJson.encodeToString(row(it, "user")) }
        val threeRows = stored.take(3).sumOf { it.length }.toLong()
        assertEquals((8L..10L).toList(), newestWindow(stored.asSequence(), 10, threeRows).map { it.seq })
        assertEquals(listOf(10L), newestWindow(stored.asSequence(), 10, 1).map { it.seq })
        assertEquals((6L..10L).toList(), newestWindow(stored.asSequence(), 5, Long.MAX_VALUE).map { it.seq })
    }

    @Test fun `an oversized newest payload stays one short row and keeps its seq`() {
        val filler = "F".repeat(PER_MESSAGE_CHARS + 1)
        val huge = WireJson.encodeToString(row(10, "user").copy(data = buildJsonObject { put("text", filler) }))
        assertTrue(huge.length > PER_MESSAGE_CHARS)
        val kept = newestWindow(sequenceOf(huge), 10, PER_MESSAGE_CHARS.toLong() + 1_000)
        assertEquals(listOf(10L), kept.map { it.seq })
        assertFalse(filler in kept.single().text)
        assertTrue(kept.single().text.length < 200)
        assertTrue(kept.single().isStoredStub)
        assertEquals(storedStubText(huge.length.toLong()), kept.single().text)
    }

    @Test fun `a normal row is not a stub and a tool row shows its output`() {
        val hello = row(1, "user").copy(data = buildJsonObject { put("text", "hello") })
        assertFalse(hello.isStoredStub)
        assertEquals("hello", hello.storedDisplayText())
        val tool = row(2, "tool").copy(data = buildJsonObject { put("output", "tool output") })
        assertEquals("tool output", tool.storedDisplayText())
    }

    @Test fun `an oversized newest row stays a stub and an older row still loads`() {
        val filler = "F".repeat(PER_MESSAGE_CHARS + 1)
        val huge = WireJson.encodeToString(row(10, "user").copy(data = buildJsonObject { put("text", filler) }))
        val older = WireJson.encodeToString(row(9, "user"))
        val kept = newestWindow(sequenceOf(huge, older), 10, PER_MESSAGE_CHARS.toLong())
        assertEquals(listOf(9L, 10L), kept.map { it.seq })
        assertFalse(filler in kept.last().text)
        assertEquals("user", kept.first().kind)
    }

    @Test fun `a normal row that does not fit is left out once the window has a row`() {
        val stored = (10L downTo 1L).map { WireJson.encodeToString(row(it, "user")) }
        val one = stored.first().length.toLong()
        assertEquals(listOf(10L), newestWindow(stored.asSequence(), 10, one).map { it.seq })
    }

    @Test fun `a huge tool row does not count toward the shown chat rows`() {
        val filler = "T".repeat(PER_MESSAGE_CHARS + 8)
        val tool = WireJson.encodeToString(row(5, "tool").copy(data = buildJsonObject { put("text", filler) }))
        val users = (4L downTo 1L).map { WireJson.encodeToString(row(it, "user")) }
        val window = newestWindow(sequenceOf(tool) + users.asSequence(), 2, Long.MAX_VALUE)
        assertEquals(listOf(3L, 4L, 5L), window.map { it.seq })
        assertEquals(2, window.count { it.isChat })
        assertEquals("tool", window.last().kind)
        assertFalse(filler in window.last().text)
    }

    @Test fun `a head without a seq is dropped instead of kept as zero`() {
        val broken = "x".repeat(PER_MESSAGE_CHARS + 1)
        val older = WireJson.encodeToString(row(4, "user"))
        assertEquals(listOf(4L), newestWindow(sequenceOf(broken, older), 10, Long.MAX_VALUE).map { it.seq })
    }

    @Test fun `capPreviews keeps the priority bot and drops a later one that does not fit`() {
        val priority = listOf(row(1, "user"), row(2, "user"))
        val later = listOf(row(3, "user"))
        assertEquals(listOf(1L, 2L), capPreviews(listOf("p" to priority, "q" to later), 2) { 1 }.map { it.seq })
        assertTrue(capPreviews(emptyList(), 10) { 1 }.isEmpty())
    }

    @Test fun `capPreviews omits a bot entirely and still keeps a later one that fits`() {
        val tooBig = listOf(row(1, "user"), row(2, "user"))
        val small = listOf(row(3, "user"))
        assertTrue(capPreviews(listOf("a" to tooBig), 3) { 2 }.isEmpty())
        assertEquals(listOf(3L), capPreviews(listOf("a" to tooBig, "b" to small), 3) { 2 }.map { it.seq })
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

    @Test fun `opening an item's card pages back until the chat holds it, never past the end or five pages`() = runBlocking {
        var pages = 0
        pageUntil(found = { pages == 2 }, more = { true }) { pages++ }
        assertEquals("found in the second page", 2, pages)
        pages = 0
        pageUntil(found = { false }, more = { pages < 1 }) { pages++ }
        assertEquals("the chat's history ended", 1, pages)
        pages = 0
        pageUntil(found = { false }, more = { true }) { pages++ }
        assertEquals("never found", 5, pages)
    }

    private fun row(seq: Long, kind: String) = Entry("e-$seq", seq, "a", rev = seq, kind = kind)
}
