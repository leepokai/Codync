package com.codync.android.core

import org.junit.Assert.*
import org.junit.Test

class WidgetFeedTest {
    @Test fun attentionSortingExcludesDeletedAndHiddenBots() {
        val bots = listOf(Bot("idle", "Idle"), Bot("hidden", "Hidden", hidden = true),
            Bot("deleted", "Deleted", deleted = true), Bot("working", "Working", status = "working"),
            Bot("approval", "Approval", status = "needsInput"))
        assertEquals(listOf("approval", "working", "idle"), widgetBots(bots).map { it.id })
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
    }
}
