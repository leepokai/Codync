package com.codync.android.core

import org.junit.Assert.*
import org.junit.Test

class MarketTest {
    @Test fun catalogPagesDeduplicate() {
        val page = CatalogPage<String>().append((1..25).map(Int::toString)) { it }
        assertEquals(25, page.items.size)
        val next = page.append(listOf("2", "26", "26")) { it }
        assertEquals(26, next.items.size)
    }

    @Test fun catalogPagingClampsAndFetchesOnlyPastLoadedRows() {
        val items = (0..46).toList()
        assertEquals(5, CatalogPaging.pageCount(47))
        assertEquals(2, CatalogPaging.pageCount(19))
        assertEquals(1, CatalogPaging.pageCount(0))
        assertEquals(9, CatalogPaging.pageCount(72, hasMore = true))
        assertEquals((0..9).toList(), CatalogPaging.slice(items, 0))
        assertEquals((40..46).toList(), CatalogPaging.slice(items, 4))
        assertEquals(emptyList<Int>(), CatalogPaging.slice(items, 9))
        assertEquals(4, CatalogPaging.clamp(9, 47))
        assertEquals(8, CatalogPaging.clamp(9, 72, hasMore = true))
        assertEquals(0, CatalogPaging.clamp(-1, 20))
        assertEquals(0, CatalogPaging.clamp(4, 0))
        assertTrue(CatalogPaging.canForward(10, 0, true))
        assertTrue(CatalogPaging.needsFetch(10, 1))
        assertTrue(CatalogPaging.canForward(20, 0, false))
        assertFalse(CatalogPaging.needsFetch(20, 1))
        assertFalse(CatalogPaging.canForward(8, 0, false))
        assertFalse(CatalogPaging.needsFetch(60, 5))
        assertTrue(CatalogPaging.needsFetch(60, 6))
        assertTrue(CatalogPaging.needsFetch(72, 7))
        assertFalse(CatalogPaging.needsFetch(80, 7))
        assertFalse(CatalogPaging.needsFetch(0, 0))
        assertFalse(CatalogPaging.needsFetch(3, 0))
        assertTrue(CatalogPaging.needsFetch(0, 1))
    }

    @Test fun connectorInputsRetainHostDefaultsAndUnknownAuthorizationDoesNotClaimSignIn() {
        val input = WireJson.decodeFromString<MarketInput>("""{"name":"REGION","required":true,"default":"provided-by-host","future":true}""")
        val option = InstallOption("local", "npm", "On computer", listOf(input))
        assertTrue(option.complete(emptyMap()))
        assertFalse(option.complete(mapOf("REGION" to "")))
        assertTrue(InstalledConnector("c", "Connector", kind = "remote", auth = "signedOut").needsSignIn)
        assertFalse(InstalledConnector("c", "Connector", kind = "remote", auth = "future").needsSignIn)
    }
}
