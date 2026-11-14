package com.codync.android

import com.codync.android.core.Bot
import com.codync.android.core.Computer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Android's counterparts of the iPhone's Shortcuts actions and Share extension: the lead "Talk to Codync" calls, and a share's bot and text. */
class ShortcutsTest {
    private fun computer(id: String, vararg bots: Bot) = AppState(Computer(id, id, id), loading = false, bots = bots.toList())
    private val mac = computer("mac", Bot("a", "Mac helper"), Bot("lead-mac", "Mac lead", lead = true))
    private val linux = computer("linux", Bot("lead-linux", "Linux lead", lead = true))

    @Test fun `talk to codync calls the picked computer's lead, else the first computer's in the list`() {
        assertEquals("lead-linux", shortcutLead(listOf(mac, linux), "linux")?.second?.id)
        assertEquals("lead-mac", shortcutLead(listOf(mac, linux), null)?.second?.id)
        // A picked computer whose lead is gone falls back to Automatic.
        assertEquals("lead-linux", shortcutLead(listOf(computer("mac", Bot("a", "Helper")), linux), "mac")?.second?.id)
        assertEquals("lead-mac", shortcutLead(listOf(linux.copy(bots = listOf(Bot("lead-linux", "Gone", lead = true, deleted = true))), mac), null)?.second?.id)
        assertNull(shortcutLead(listOf(computer("mac", Bot("a", "Helper"))), null))
        // Settings lists the same leads the shortcut picks from.
        assertEquals(listOf("lead-mac", "lead-linux"), leads(listOf(mac, linux)).map { it.second.id })
    }

    @Test fun `shared text is cut at a character to what a message holds`() {
        assertEquals("hello", fitMessage("hello"))
        // 30,000 Chinese characters are about 90 KB of UTF-8, three bytes each: whole ones up to 64 KiB.
        val chinese = fitMessage("字".repeat(30_000))
        assertEquals(DraftBook.TEXT_BYTES / 3, chinese.length)
        assertTrue(chinese.toByteArray().size <= DraftBook.TEXT_BYTES)
        // An emoji (two UTF-16 units, four bytes) is kept whole or left out.
        val typed = "a".repeat(DraftBook.TEXT_BYTES - 6)
        assertEquals(typed + "😀", fitMessage(typed + "😀😀"))
    }

    @Test fun `shared text goes as pasted only while the message still holds it`() {
        val article = SharedContent("Rust 2024 is out\nhttps://example.com/rust", emptyList())
        assertEquals(listOf(article.text), article.pasted("Read this:\n" + article.text))
        assertEquals(emptyList<String>(), article.pasted("Rust 2024 is out"))
        assertEquals(emptyList<String>(), SharedContent(" ", emptyList()).pasted("typed"))
    }

    @Test fun `a share's bot becomes the most recent, each once, three at most`() {
        val (a, b, c, d) = listOf("a", "b", "c", "d").map { ShareRecipient("mac", it) }
        assertEquals(listOf(a), emptyList<ShareRecipient>().recorded(a))
        assertEquals(listOf(b, a), listOf(a).recorded(b))
        assertEquals(listOf(a, c, b), listOf(c, b, a).recorded(a))
        assertEquals(listOf(d, c, b), listOf(c, b, a).recorded(d))
    }

    @Test fun `the share sheet lists Recent's bots still there, then the rest of each computer`() {
        val group = Bot("team", "Team", kind = "group", members = listOf("a"))
        val empty = Bot("nobody", "Empty group", kind = "group")
        val hidden = Bot("hidden", "Hidden", hidden = true)
        val mac = computer("mac", Bot("a", "Helper"), Bot("b", "Writer"), group, empty, hidden)
        val linux = computer("linux", Bot("x", "Builder"))
        val recipients = listOf(ShareRecipient("linux", "x"), ShareRecipient("mac", "gone"), ShareRecipient("mac", "b"), ShareRecipient("mac", "hidden"))
        val (recent, sections) = shareChoices(listOf(mac, linux), recipients)
        assertEquals(listOf("x", "b"), recent.map { it.bot.id })
        assertEquals(listOf("mac" to listOf("a", "team"), "linux" to emptyList()),
            sections.map { (state, choices) -> state.computer.id to choices.map { it.bot.id } })
    }
}
