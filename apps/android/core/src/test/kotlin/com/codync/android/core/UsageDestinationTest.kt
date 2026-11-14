package com.codync.android.core

import org.junit.Assert.*
import org.junit.Test

class UsageDestinationTest {
    private val query = "scope=local&computer=AAAAAAAAAAAAAAAAAAAAAA"
    @Test fun usageLinksRetainTheirAccountAndComputerScope() {
        val destination = UsageDestination.parse("codync://usage?$query", "codync")
        assertEquals("local", destination.scope)
        assertEquals("AAAAAAAAAAAAAAAAAAAAAA", destination.computerId)
    }
    @Test fun incompleteAmbiguousOrForeignLinksAreRejected() {
        for (value in listOf("codync://usage", "codync://usage/path?$query", "codync://usage?$query&scope=local",
            "codync://usage?$query#other", "https://usage?$query")) {
            assertTrue(value, runCatching { UsageDestination.parse(value, "codync") }.isFailure)
        }
    }
}
