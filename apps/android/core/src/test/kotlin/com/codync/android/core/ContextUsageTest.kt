package com.codync.android.core

import kotlinx.serialization.encodeToString
import org.junit.Assert.*
import org.junit.Test

class ContextUsageTest {
    @Test fun `unknown context use is distinct from a reported empty session`() {
        assertNull(WireJson.decodeFromString<Bot>("""{"id":"b"}""").contextUsage)
        assertNull(WireJson.decodeFromString<Bot>("""{"id":"b","contextUsage":null}""").contextUsage)
        val empty = WireJson.decodeFromString<Bot>("""{"id":"b","contextUsage":{"usedTokens":0,"maxTokens":200000,"percent":0,"level":"normal"}}""").contextUsage!!
        assertEquals(0L, empty.usedTokens)
        assertEquals(200000L, empty.maxTokens)
        assertEquals(0, empty.percent)
    }

    @Test fun `context snapshot preserves host percentage and future levels`() {
        val bot = WireJson.decodeFromString<Bot>("""{"id":"b","contextUsage":{"usedTokens":170000,"maxTokens":200000,"percent":85,"level":"future"}}""")
        val usage = requireNotNull(bot.contextUsage)
        assertEquals(170000L, usage.usedTokens)
        assertEquals(85, usage.percent)
        assertEquals("future", usage.level)
        assertEquals(bot.contextUsage, WireJson.decodeFromString<Bot>(WireJson.encodeToString(bot)).contextUsage)
    }
}
