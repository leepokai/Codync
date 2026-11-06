package com.codync.android

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class ConnectorCatalogTest {
    @Test fun moreConnectorsReadOnPastEmptyPagesAndStopAtAFailure() = runBlocking {
        val asked = mutableListOf<String?>()
        var answer: (String?) -> Pair<List<Int>, String?> = { cursor ->
            when (cursor) {
                null -> (0 until 15).toList() to "a"
                "a" -> emptyList<Int>() to "b"
                else -> (15 until 25).toList() to "c"
            }
        }
        val catalog = ConnectorCatalog<Int>({ it.toString() }) { _, cursor -> asked += cursor; answer(cursor) }
        catalog.load("")
        assertEquals(15, catalog.items.items.size)

        // The next page has nothing to install but moves the cursor on: the one after it brings the rows.
        catalog.loadMore()
        assertEquals(15, catalog.items.items.size)
        assertEquals("b", catalog.cursor)
        catalog.loadMore()
        assertEquals(listOf(null, "a", "b"), asked)
        assertEquals(25, catalog.items.items.size)
        assertEquals("c", catalog.cursor)

        answer = { error("offline") }
        catalog.loadMore()
        assertEquals(25, catalog.items.items.size)
        assertEquals("c", catalog.cursor)
        assertEquals("offline", catalog.error)
        assertFalse(catalog.loadingMore)

        // The cursor ends: nothing more to ask for.
        answer = { listOf(25) to null }
        catalog.loadMore()
        assertEquals(26, catalog.items.items.size)
        assertFalse(catalog.hasMore)
        assertNull(catalog.error)
    }
}
