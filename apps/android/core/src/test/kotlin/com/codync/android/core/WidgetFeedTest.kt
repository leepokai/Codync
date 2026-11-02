package com.codync.android.core

import org.junit.Assert.*
import org.junit.Test

class WidgetFeedTest {
    @Test fun attentionSortingExcludesDeletedAndHiddenBots() {
        val bots = listOf(Bot("idle", "Idle"), Bot("hidden", "Hidden", hidden = true),
            Bot("deleted", "Deleted", deleted = true), Bot("working", "Working", status = "working"),
            Bot("approval", "Approval", status = "needsInput"))
        assertEquals(listOf("approval", "working", "idle"), widgetBots(mapOf("mac" to bots)).map { it.id })
    }

    @Test fun `every computer's bots share one order and keep the computer they live on`() {
        val bots = widgetBots(mapOf("mac" to listOf(Bot("a", "Mac idle"), Bot("b", "Mac working", status = "working")),
            "linux" to listOf(Bot("a", "Linux approval", status = "needsInput"))))
        assertEquals(listOf("linux/a", "mac/b", "mac/a"), bots.map { it.key })
        // The same bot ID on two computers stays two spots.
        assertEquals(listOf("mac/a", "linux/a", "mac/b", null), teamSlots(bots, listOf("mac/a", null, null, null)).map { it?.key })
        // A pick saved before several computers (a bare bot ID) still holds its spot.
        assertEquals(listOf("mac/b", "linux/a", "mac/a", null), teamSlots(bots, listOf("b", null, null, null)).map { it?.key })
    }

    @Test fun `an unreachable computer keeps the bots it showed and a forgotten one takes its bots along`() {
        val shown = WidgetFeed(bots = listOf(WidgetBot("a", "Old", "idle", computerId = "mac"), WidgetBot("b", "Gone", "idle", computerId = "old"),
            WidgetBot("c", "Stale", "idle", computerId = "linux")), botsAt = 5)
        val next = shown.readingBots(listOf("mac" to null, "linux" to listOf(Bot("d", "New", status = "working"))), fresh = false, now = 9)
        assertEquals(listOf("linux/d", "mac/a"), next.bots.map { it.key })
        assertEquals(5L, next.botsAt)
        assertEquals(9L, shown.readingBots(listOf("linux" to emptyList()), fresh = true, now = 9).botsAt)
        assertTrue(shown.readingBots(emptyList(), fresh = true, now = 9).let { it.bots.isEmpty() && it.botsAt == 5L })
    }

    @Test fun `team spots keep picks, skip deleted ones and fill the rest in feed order`() {
        val bots = listOf(WidgetBot("a", "A", "needsInput"), WidgetBot("b", "B", "working"), WidgetBot("c", "C", "idle"))
        assertEquals(listOf("c", "a", "b", null), teamSlots(bots, listOf("c", "gone", null, null)).map { it?.id })
        assertEquals(listOf("a", "b", "c", null), teamSlots(bots, emptyList()).map { it?.id })
    }

    @Test fun `usage limits lead with the tightest limit of any provider and keep the rest in order`() {
        val report = UsageReport(listOf(
            UsageProvider("claude", "Claude", listOf(UsageWindow("five_hour", "5-hour", 16.0), UsageWindow("seven_day", "Weekly", 59.0))),
            UsageProvider("codex", "Codex", listOf(UsageWindow("primary", "5-hour", 8.0), UsageWindow("secondary", "Weekly", null)))))
        assertEquals(listOf("Claude Weekly", "Claude 5-hour", "Codex 5-hour", "Codex Weekly"),
            usageLimits(report).map { "${it.provider} ${it.window.label}" })
        assertEquals(listOf("Codex Weekly"), usageLimits(UsageReport(listOf(UsageProvider("codex", "Codex",
            listOf(UsageWindow("secondary", "Weekly")))))).map { "${it.provider} ${it.window.label}" })
        assertTrue(usageLimits(UsageReport()).isEmpty())
        val noted = UsageReport(listOf(UsageProvider("vibe", "Mistral Vibe", listOf(UsageWindow("cost", "Cost today", 0.0, text = "$1.50")))))
        assertTrue(usageLimits(noted).isEmpty())
    }

    @Test fun `the most urgent bot is the first at work and nobody when all are quiet`() {
        val bots = listOf(WidgetBot("a", "A", "needsInput"), WidgetBot("b", "B", "working"), WidgetBot("c", "C", "error"))
        assertEquals("a", mostUrgentBot(bots)?.id)
        assertEquals("b", mostUrgentBot(bots.drop(1))?.id)
        assertNull(mostUrgentBot(bots.drop(2)))
        // Two bots that need you equally: the most recent, pinned or not (iOS `mostUrgent`).
        val tie = listOf(WidgetBot("pinned", "P", "needsInput", lastAt = 1), WidgetBot("recent", "R", "needsInput", lastAt = 2))
        assertEquals("recent", mostUrgentBot(tie)?.id)
    }
}
