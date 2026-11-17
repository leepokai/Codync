package com.codync.android

import com.codync.android.core.Bot
import com.codync.android.core.Computer
import com.codync.android.core.Entry
import com.codync.android.core.MirrorEvent
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Test

class ReadingTest {
    private fun message(kind: String = "user", bot: String = "bot") =
        MirrorEvent.EntryChanged(Entry("e", 1, bot, 1, kind, data = buildJsonObject { put("text", "hi") }))

    @Test fun `the chat stays read under Full conversation`() {
        assertEquals(listOf("bot"), AppState(Computer("fixture", "Studio", "fixture"), selectedBot = "bot", trace = true).readingChats)
        assertEquals(emptyList<String>(), AppState(Computer("fixture", "Studio", "fixture")).readingChats)
    }

    @Test fun `a message is read where it lands, and a bot counting unread reads all it shows`() {
        val chats = listOf("bot")
        assertEquals(chats, readAfter(listOf(message()), chats))
        assertEquals(emptyList<String>(), readAfter(listOf(message(kind = "tool"), message(bot = "other")), chats))
        assertEquals(chats, readAfter(listOf(MirrorEvent.BotChanged(Bot("bot", unread = 2))), chats))
        assertEquals(emptyList<String>(), readAfter(listOf(MirrorEvent.BotChanged(Bot("bot", unread = 0))), chats))
    }

    @Test fun `a chat left with nothing unread is read, by its last word in the batch`() {
        fun changed(bot: String, unread: Int) = MirrorEvent.BotChanged(Bot(bot, unread = unread))
        assertEquals(listOf("read"), readChats(listOf(changed("read", 0), changed("unread", 1), message())))
        assertEquals(emptyList<String>(), readChats(listOf(changed("bot", 0), changed("bot", 1))))
        assertEquals(listOf("bot"), readChats(listOf(changed("bot", 1), changed("bot", 0))))
    }
}
