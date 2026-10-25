package com.codync.android

import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class ConnectorCatalogTest {
    @Test fun aConnectorPageFillsUntilFullAndFallsBackWhenItCannot() = runBlocking {
        val asked = mutableListOf<String?>()
        var offline = false
        val catalog = ConnectorCatalog<Int>({ it.toString() }) { _, cursor ->
            asked += cursor
            if (offline) error("offline")
            when (cursor) {
                null -> (0 until 15).toList() to "a"
                "a" -> (15 until 18).toList() to "b"
                else -> (18 until 25).toList() to "c"
            }
        }
        catalog.load("")
        assertEquals(15, catalog.items.items.size)

        // Page 2 needs rows 10-19: two registry pages fill it.
        catalog.show(1, 10)
        catalog.fill(10)
        assertEquals(listOf(null, "a", "b"), asked)
        assertEquals(1, catalog.page)
        assertEquals(25, catalog.items.items.size)
        assertEquals("c", catalog.cursor)

        // The page past the loaded rows fails: back to the last page with rows.
        offline = true
        catalog.show(3, 10)
        assertEquals(3, catalog.page)
        catalog.fill(10)
        assertEquals(2, catalog.page)
        assertEquals("offline", catalog.error)
        assertFalse(catalog.loadingMore)
    }
}
