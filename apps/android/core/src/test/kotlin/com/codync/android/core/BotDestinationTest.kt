package com.codync.android.core

import org.junit.Assert.*
import org.junit.Test

class BotDestinationTest {
    private val base = "codync://bot/a-b?scope=local&computer=9ESVKkTXlLyIExxnW7-PIg"
    @Test fun `links retain all three destination boundaries`() {
        assertEquals(BotDestination("local", "9ESVKkTXlLyIExxnW7-PIg", "a-b"), BotDestination.parse(base, "codync"))
        assertEquals("a b", BotDestination.parse(base.replace("/a-b?", "/a%20b?"), "codync").botId)
        val scope = "a".repeat(64)
        assertEquals(scope, BotDestination.parse(base.replace("scope=local", "scope=$scope"), "codync").scope)
    }
    @Test fun `missing duplicate and ambiguous destinations fail instead of choosing another account`() {
        for (url in listOf(base.replace("scope=local&", ""), base + "&scope=local", base + "&computer=bad",
            base.replace("computer=9ESVKkTXlLyIExxnW7-PIg", "computer=bad"), base.replace("/a-b?", "/a%2Fb?"),
            base.replace("/a-b?", "/?"), base + "#fragment", base.replace("://bot/", "://other@bot/"),
            base.replace("scope=local", "scope=other"))) {
            assertFails<IllegalArgumentException> { BotDestination.parse(url, "codync") }
        }
    }
}
