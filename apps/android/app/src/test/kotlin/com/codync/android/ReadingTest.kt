package com.codync.android

import com.codync.android.core.Bot
import com.codync.android.core.Computer
import com.codync.android.core.Entry
import com.codync.android.core.MirrorEvent
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
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

    @Test fun `mark as unread is offered for a read chat on a host that has it`() {
        val computer = Computer("fixture", "Studio", "fixture")
        val host = AppState(computer, hello = buildJsonObject { put("version", "2.41.0") })
        assertEquals(true, host.offersMarkUnread(Bot("bot")))
        assertEquals(false, host.offersMarkUnread(Bot("bot", unread = 1)))
        assertEquals(false, host.offersMarkUnread(Bot("bot", keptUnread = true)))
        assertEquals(false, AppState(computer, hello = buildJsonObject { put("version", "2.40.0") }).offersMarkUnread(Bot("bot")))
        assertEquals(false, AppState(computer).offersMarkUnread(Bot("bot")))
    }

    @Test fun `a read on screen says it's automatic, and only that one`() {
        assertEquals(buildJsonObject { put("botId", "a"); put("automatic", true) }, readBody("a", automatic = true))
        assertEquals(buildJsonObject { put("botId", "a") }, readBody("a"))
    }

    /** Read marks over actions that run at once, like the store's on the main thread; an error lands in [shown]. */
    private class Marks(private val starts: () -> Boolean = { true }) {
        val shown = mutableListOf<Exception>()
        val reads = ReadMarks { _, operation ->
            starts().also { started ->
                if (started) CoroutineScope(Dispatchers.Unconfined).launch {
                    try { operation() } catch (error: Exception) { shown.add(error) }
                }
            }
        }
    }

    @Test fun `an open that fails stays quiet and goes again once, the next time the chat is on screen`() {
        val marks = Marks()
        var sends = 0
        marks.reads.open("a") { sends++; error("no link") }
        assertEquals(emptyList<Exception>(), marks.shown)
        assertEquals(true, marks.reads.retryOpen("a") { sends++ })
        assertEquals(2, sends)
        assertEquals(false, marks.reads.retryOpen("a") { sends++ })
        assertEquals(2, sends)
    }

    @Test fun `an open that never started goes again too`() {
        var loading = true
        val marks = Marks { !loading }
        marks.reads.open("a") { error("not while loading") }
        loading = false
        var sent = false
        assertEquals(true, marks.reads.retryOpen("a") { sent = true })
        assertEquals(true, sent)
    }

    @Test fun `an open waiting behind another one has not failed`() {
        val marks = Marks()
        val first = CompletableDeferred<Unit>()
        val sent = mutableListOf<String>()
        marks.reads.open("a") { first.await(); sent.add("first") }
        marks.reads.open("a") { sent.add("second") }
        assertEquals(false, marks.reads.retryOpen("a") { sent.add("again") })
        assertEquals(emptyList<String>(), sent)
        first.complete(Unit)
        assertEquals(listOf("first", "second"), sent)
        assertEquals(false, marks.reads.retryOpen("a") { sent.add("again") })
    }

    @Test fun `an open that fails while a newer mark waits behind it does not go again`() {
        val marks = Marks()
        val first = CompletableDeferred<Unit>()
        marks.reads.open("a") { first.await(); error("no link") }
        marks.reads.mark("a") {}
        first.complete(Unit)
        assertEquals(false, marks.reads.retryOpen("a") {})
    }

    @Test fun `marking a chat drops an open that never reached the host`() {
        val marks = Marks()
        marks.reads.open("a") { error("no link") }
        marks.reads.mark("a") {}
        assertEquals(false, marks.reads.retryOpen("a") {})
    }

    @Test fun `a mark waits for the one before it on the same chat, and no other`() {
        val marks = Marks()
        val first = CompletableDeferred<Unit>()
        val sent = mutableListOf<String>()
        marks.reads.mark("a") { first.await(); sent.add("read") }
        marks.reads.mark("a") { sent.add("unread") }
        marks.reads.mark("b") { sent.add("other chat") }
        assertEquals(listOf("other chat"), sent)
        first.complete(Unit)
        assertEquals(listOf("other chat", "read", "unread"), sent)
    }

    @Test fun `a mark that fails shows its error and lets the next one go`() {
        val marks = Marks()
        val sent = mutableListOf<String>()
        marks.reads.mark("a") { error("refused") }
        marks.reads.mark("a") { sent.add("next") }
        assertEquals(listOf("refused"), marks.shown.map { it.message })
        assertEquals(listOf("next"), sent)
    }
}
