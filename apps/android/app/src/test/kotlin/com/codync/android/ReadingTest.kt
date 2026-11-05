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

    @Test fun `a bot the host holds unread is picked out of a batch`() {
        val held = MirrorEvent.BotChanged(Bot("held", unread = 1, keptUnread = true))
        assertEquals(listOf("held"), keptUnreadIn(listOf(held, MirrorEvent.BotChanged(Bot("bot", unread = 1)), message())))
    }

    @Test fun `catch-up ends on its last replayed event even when the host rev is ahead of all of them`() {
        val hello = buildJsonObject { put("type", "hello"); put("rev", 9); put("catchUp", 2) }
        val icons = buildJsonObject { put("type", "icons") }
        val bot = buildJsonObject { put("type", "bot"); put("rev", 3) }
        val count = CatchUpCount()
        assertEquals(listOf(false, false, true, false), listOf(hello, icons, bot, bot).map(count::ended))
        assertEquals(false, CatchUpCount().ended(bot))
        assertEquals(true, CatchUpCount().ended(buildJsonObject { put("type", "hello"); put("catchUp", 0) }))
    }

    @Test fun `reads wait from a counted hello until its last replayed event, and never for an uncounted one`() {
        val hello = buildJsonObject { put("type", "hello"); put("catchUp", 2) }
        val bot = buildJsonObject { put("type", "bot") }
        val count = CatchUpCount()
        assertEquals(listOf(true, true, false), listOf(hello, bot, bot).map { count.ended(it); count.replaying })
        val uncounted = CatchUpCount()
        uncounted.ended(buildJsonObject { put("type", "hello") })
        assertEquals(false, uncounted.replaying)
    }

    @Test fun `mark as unread is offered for a read chat on a host that has it`() {
        val host = AppState(hello = buildJsonObject { put("version", "2.41.0") })
        assertEquals(true, host.offersMarkUnread(Bot("bot")))
        assertEquals(false, host.offersMarkUnread(Bot("bot", unread = 1)))
        assertEquals(false, host.offersMarkUnread(Bot("bot", keptUnread = true)))
        assertEquals(false, AppState(hello = buildJsonObject { put("version", "2.40.0") }).offersMarkUnread(Bot("bot")))
        assertEquals(false, AppState().offersMarkUnread(Bot("bot")))
    }
}
