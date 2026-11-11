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
        MirrorEvent.EntryChanged(Entry("e", 1, bot, 1, kind, data = buildJsonObject { put("text", "hi") }, threadId = thread))

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

    @Test fun `the new line sits above the first message past the read mark`() {
        fun item(rev: Long) = ChatItem.Message("e$rev", Entry("e$rev", rev, "bot", rev, "agent", data = buildJsonObject { put("text", "hi") }), false)
        val items: List<ChatItem> = listOf(item(1), item(2), item(3))
        assertEquals(listOf("e1", "new-e2", "e2", "e3"), withUnreadLine(items, 1).map { it.id })
        assertEquals("nothing newer than the mark", items, withUnreadLine(items, 3))
        assertEquals(items, withUnreadLine(items, null))
        assertEquals(listOf("new-e1", "e1", "e2", "e3"), withUnreadLine(items, 0).map { it.id })
    }
}
