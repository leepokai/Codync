package com.codync.android

import com.codync.android.core.Bot
import com.codync.android.core.Computer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** Android's counterparts of the iPhone's Shortcuts actions: the lead "Talk to Codync" calls, and shared text in a draft. */
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

    @Test fun `shared text goes after what's typed, cut at a character to what a draft holds`() {
        assertEquals("typed hello", sharedDraft("typed", "hello"))
        assertEquals("typed\nhello", sharedDraft("typed\n", "hello"))
        assertEquals("hello", sharedDraft("", "hello"))
        // 30,000 Chinese characters are about 90 KB of UTF-8, three bytes each: whole ones up to 64 KiB.
        val chinese = sharedDraft("", "字".repeat(30_000))
        assertEquals(DraftBook.TEXT_BYTES / 3, chinese.length)
        assertTrue(chinese.toByteArray().size <= DraftBook.TEXT_BYTES)
        // An emoji (two UTF-16 units, four bytes) is kept whole or left out, after the typed text.
        val typed = "a".repeat(DraftBook.TEXT_BYTES - 6)
        assertEquals("$typed \uD83D\uDE00", sharedDraft(typed, "\uD83D\uDE00\uD83D\uDE00"))
    }
}
