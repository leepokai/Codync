package com.codync.android

import com.codync.android.core.Bot
import com.codync.android.core.Computer
import com.codync.android.core.Entry
import com.codync.android.core.MirrorEvent
import com.codync.android.core.Outside
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
        // A card still waiting isn't something to read (#366).
        assertEquals(emptyList<String>(), readAfter(listOf(MirrorEvent.BotChanged(Bot("bot", unread = 1, waiting = 1))), chats))
    }

    @Test fun `an alert goes once its chat has dealt with it, here or elsewhere`() {
        fun changed(bot: String, unread: Int, status: String = "idle") = MirrorEvent.BotChanged(Bot(bot, unread = unread, status = status))
        val alerts = mapOf("reply" to "done", "question" to "needsInput", "failure" to "failed")
        // A reply read on the computer: only the end state reaches the phone.
        assertEquals(listOf("reply"), readChats(listOf(changed("reply", 0), message()), alerts))
        assertEquals(emptyList<String>(), readChats(listOf(changed("reply", 0), changed("reply", 1)), alerts))
        // A question stays while the bot waits on it, and goes once answered.
        assertEquals(emptyList<String>(), readChats(listOf(changed("question", 0, "needsInput")), alerts))
        assertEquals(listOf("question"), readChats(listOf(changed("question", 0, "working")), alerts))
        val threadAsking = Bot("question", unread = 0, status = "idle", outside = Outside(status = "needsInput"))
        assertEquals(emptyList<String>(), readChats(listOf(MirrorEvent.BotChanged(threadAsking)), alerts))
        // A failure never counted unread; a chat with no alert has nothing to clear.
        assertEquals(emptyList<String>(), readChats(listOf(changed("failure", 0), changed("none", 0)), alerts))
    }
}
