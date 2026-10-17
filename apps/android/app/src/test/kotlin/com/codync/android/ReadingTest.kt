package com.codync.android

import com.codync.android.core.Bot
import com.codync.android.core.Entry
import com.codync.android.core.MirrorEvent
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Test

class ReadingTest {
    private val main = ReadingScope("bot", null)
    private val thread = ReadingScope("bot", "root")

    private fun message(thread: String?, kind: String = "user", bot: String = "bot") =
        MirrorEvent.EntryChanged(Entry("e", 1, bot, thread, 1, kind, data = buildJsonObject { put("text", "hi") }))

    @Test fun `the chat stays read under its thread and Full conversation`() {
        assertEquals(listOf(main, thread), AppState(selectedBot = "bot", thread = "root", trace = true).readingScopes)
        assertEquals(listOf(main), AppState(selectedBot = "bot").readingScopes)
        assertEquals(emptyList<ReadingScope>(), AppState().readingScopes)
    }

    @Test fun `a message is read where it lands, and a bot counting unread reads all it shows`() {
        val scopes = listOf(main, thread)
        assertEquals(listOf(thread), readAfter(listOf(message("root")), scopes))
        assertEquals(listOf(main), readAfter(listOf(message(null)), scopes))
        assertEquals(emptyList<ReadingScope>(), readAfter(listOf(message(null, kind = "tool"), message(null, bot = "other")), scopes))
        assertEquals(scopes, readAfter(listOf(MirrorEvent.BotChanged(Bot("bot", unread = 2))), scopes))
        assertEquals(emptyList<ReadingScope>(), readAfter(listOf(MirrorEvent.BotChanged(Bot("bot", unread = 0))), scopes))
    }
}
