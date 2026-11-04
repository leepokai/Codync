package com.codync.android.core

import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.*
import org.junit.Test

class WidgetOverviewTest {
    private val now = 10_000_000L
    private val computer = "AAAAAAAAAAAAAAAAAAAAAA"

    private fun bot(id: String, status: String = "idle", at: Long = 0) = WidgetBot(id, id.uppercase(), status, lastAt = at)
    private val usage = UsageReport(listOf(UsageProvider("claude", "Claude",
        listOf(UsageWindow("five_hour", "5-hour", 16.0), UsageWindow("seven_day", "Weekly", 59.0)))))

    private fun feed(bots: List<WidgetBot>, botsAt: Long = now, withUsage: Boolean = true, offline: Long = 0, approvals: List<WidgetApproval> = emptyList()) =
        WidgetFeed(computerId = computer, computerName = "Studio", bots = bots, botsAt = botsAt,
            usage = if (withUsage) usage else UsageReport(), usageAt = botsAt, approvals = approvals, offlineSince = offline)

    private val busy = feed(listOf(bot("a", "needsInput", 5), bot("b", "working", 9), bot("c", "idle", 7), bot("d", "error", 3), bot("e", "idle")))
    private fun sections(plan: OverviewPlan) = plan.columns.map { column -> column.map { it.section } }

    @Test fun `the size class follows the sizes the widget declares`() {
        assertEquals(WidgetSizeClass.Small, widgetSizeClass(110f, 110f))
        assertEquals(WidgetSizeClass.Medium, widgetSizeClass(250f, 110f))
        assertEquals(WidgetSizeClass.Medium, widgetSizeClass(110f, 250f))
        assertEquals(WidgetSizeClass.Large, widgetSizeClass(250f, 250f))
        assertEquals(WidgetSizeClass.ExtraLarge, widgetSizeClass(400f, 250f))
        assertEquals(WidgetSizeClass.Large, widgetSizeClass(379f, 400f))
    }

    @Test fun `small keeps the status alone and medium adds the first section with something to say`() {
        val config = OverviewConfig()
        assertEquals(listOf(listOf(WidgetSection.Status)), sections(overviewPlan(busy, config, WidgetSizeClass.Small, now)))
        assertEquals(listOf(listOf(WidgetSection.Status, WidgetSection.Attention)), sections(overviewPlan(busy, config, WidgetSizeClass.Medium, now)))
        val quiet = feed(listOf(bot("a", "working"), bot("b")))
        assertEquals(listOf(listOf(WidgetSection.Status, WidgetSection.Team)), sections(overviewPlan(quiet, config, WidgetSizeClass.Medium, now)))
        val usageFirst = config.copy(sections = listOf(WidgetSection.Status, WidgetSection.Usage, WidgetSection.Attention))
        assertEquals(listOf(listOf(WidgetSection.Status, WidgetSection.Usage)), sections(overviewPlan(busy, usageFirst, WidgetSizeClass.Medium, now)))
    }

    @Test fun `large fills one column in the chosen order within its row budget`() {
        val plan = overviewPlan(busy, OverviewConfig(), WidgetSizeClass.Large, now)
        assertEquals(1, plan.columns.size)
        assertEquals(listOf(WidgetSection.Status, WidgetSection.Attention, WidgetSection.Team, WidgetSection.Usage, WidgetSection.Recent), plan.sections.map { it.section })
        assertEquals(2, plan.sections.first { it.section == WidgetSection.Attention }.rows)
        assertTrue(plan.sections.sumOf { it.rows } <= 8)
        val custom = OverviewConfig(sections = listOf(WidgetSection.Usage, WidgetSection.Status))
        assertEquals(listOf(WidgetSection.Status, WidgetSection.Usage), overviewPlan(busy, custom, WidgetSizeClass.Large, now).sections.map { it.section })
    }

    @Test fun `extra large uses two columns and compact density fits more rows`() {
        val plan = overviewPlan(busy, OverviewConfig(), WidgetSizeClass.ExtraLarge, now)
        assertEquals(2, plan.columns.size)
        assertEquals(WidgetSection.Status, plan.columns[0].first().section)
        assertEquals(5, plan.sections.map { it.section }.toSet().size)
        val many = feed((1..6).map { bot("n$it", "needsInput") })
        val comfortable = overviewPlan(many, OverviewConfig(sections = listOf(WidgetSection.Attention)), WidgetSizeClass.Medium, now)
        val compact = overviewPlan(many, OverviewConfig(sections = listOf(WidgetSection.Attention), density = WidgetDensity.Compact), WidgetSizeClass.Medium, now)
        assertTrue(compact.sections.sumOf { it.rows } >= comfortable.sections.sumOf { it.rows })
    }

    @Test fun `a widget with nothing to show still shows the status`() {
        val empty = feed(emptyList(), withUsage = false)
        for (size in WidgetSizeClass.entries) assertEquals(listOf(WidgetSection.Status), overviewPlan(empty, OverviewConfig(), size, now).sections.map { it.section })
        val onlyUsage = OverviewConfig(sections = listOf(WidgetSection.Usage))
        assertEquals(listOf(WidgetSection.Status), overviewPlan(empty, onlyUsage, WidgetSizeClass.Medium, now).sections.map { it.section })
    }

    @Test fun `old or unreachable numbers are never shown as current`() {
        assertEquals(OverviewState.Fresh, overviewState(busy, OverviewConfig(), now))
        val aging = busy.copy(botsAt = now - 20 * 60_000)
        assertEquals(OverviewState.Aging, overviewState(aging, OverviewConfig(), now))
        assertTrue(overviewPlan(aging, OverviewConfig(), WidgetSizeClass.Large, now).sections.size > 1)
        val stale = busy.copy(botsAt = now - 2 * 3_600_000)
        assertEquals(OverviewState.Stale, overviewState(stale, OverviewConfig(), now))
        assertEquals(listOf(WidgetSection.Status), overviewPlan(stale, OverviewConfig(), WidgetSizeClass.ExtraLarge, now).sections.map { it.section })
        val offline = busy.copy(offlineSince = now - 1000)
        assertEquals(OverviewState.Offline, overviewState(offline, OverviewConfig(), now))
        assertEquals(listOf(WidgetSection.Status), overviewPlan(offline, OverviewConfig(), WidgetSizeClass.Large, now).sections.map { it.section })
        assertEquals(OverviewState.NoData, overviewState(WidgetFeed(computerId = computer), OverviewConfig(), now))
        assertEquals(OverviewState.NoComputer, overviewState(WidgetFeed(), OverviewConfig(), now))
        assertEquals(40L, reportAgeMinutes(busy.copy(botsAt = now - 40 * 60_000), now))
        assertNull(reportAgeMinutes(WidgetFeed(), now))
    }

    @Test fun `a widget follows its computer and only the bots it was set to`() {
        assertEquals(OverviewState.NoComputer, overviewState(busy, OverviewConfig(computerId = "BBBBBBBBBBBBBBBBBBBBBB"), now))
        val picked = OverviewConfig(botIds = listOf("b"))
        assertEquals(listOf("b"), scopedFeed(busy, picked).bots.map { it.id })
        assertTrue(overviewPlan(busy, picked, WidgetSizeClass.Large, now).sections.none { it.section == WidgetSection.Attention })
    }

    @Test fun `settings round trip and a damaged value falls back to the defaults`() {
        val config = OverviewConfig(listOf(WidgetSection.Usage, WidgetSection.Usage, WidgetSection.Team), computer, listOf("a", "a", "b"), WidgetTheme.Dark, WidgetDensity.Compact)
        val back = OverviewConfig.decode(config.encode())
        assertEquals(listOf(WidgetSection.Usage, WidgetSection.Team), back.sections)
        assertEquals(listOf("a", "b"), back.botIds)
        assertEquals(WidgetTheme.Dark, back.theme)
        assertEquals(OverviewConfig(), OverviewConfig.decode("{broken"))
        assertEquals(OverviewConfig(), OverviewConfig.decode(null))
        assertEquals(OverviewConfig.DEFAULT_SECTIONS, OverviewConfig(sections = emptyList()).normalized().sections)
        assertEquals(OverviewConfig.MAX_BOTS, OverviewConfig(botIds = (1..100).map { "b$it" }).normalized().botIds.size)
    }

    // MARK: actions

    private fun approval(risk: String, at: Long = now, allow: String? = "allow_once", deny: String? = "reject_once") =
        WidgetApproval("bot", "entry1", allow, deny, risk, at)

    @Test fun `a request that changes the computer is never allowed from the widget`() {
        for (kind in listOf("execute", "edit", "delete", "move", "other", null, "")) {
            assertEquals("risk of $kind", "high", WidgetActions.riskOf(kind))
            assertEquals(ApprovalPlan.OpenApp, WidgetActions.approval(approval(WidgetActions.riskOf(kind)), allow = true, now = now))
        }
        for (kind in listOf("read", "search", "fetch", "think")) assertEquals("low", WidgetActions.riskOf(kind))
    }

    @Test fun `a low risk request is allowed only while the report is recent`() {
        assertEquals(ApprovalPlan.Send("allow_once"), WidgetActions.approval(approval("low"), allow = true, now = now + 60_000))
        assertEquals(ApprovalPlan.OpenApp, WidgetActions.approval(approval("low"), allow = true, now = now + 3 * 60_000))
        assertEquals(ApprovalPlan.OpenApp, WidgetActions.approval(approval("low", at = 0), allow = true, now = now))
        assertEquals(ApprovalPlan.OpenApp, WidgetActions.approval(approval("low", allow = null), allow = true, now = now))
        assertEquals(ApprovalPlan.OpenApp, WidgetActions.approval(approval("low", at = now + 5000), allow = true, now = now))
    }

    @Test fun `denying is always one tap, also for what changes the computer and for old reports`() {
        assertEquals(ApprovalPlan.Send("reject_once"), WidgetActions.approval(approval("high"), allow = false, now = now))
        assertEquals(ApprovalPlan.Send("reject_once"), WidgetActions.approval(approval("high", at = 1), allow = false, now = now + 99_999_999))
        assertEquals(ApprovalPlan.OpenApp, WidgetActions.approval(approval("high", deny = null), allow = false, now = now))
        assertEquals(ApprovalPlan.OpenApp, WidgetActions.approval(null, allow = false, now = now))
    }

    @Test fun `a malformed request or option never reaches the host`() {
        assertEquals(ApprovalPlan.OpenApp, WidgetActions.approval(approval("low").copy(entryId = "bad id!"), allow = true, now = now))
        assertEquals(ApprovalPlan.OpenApp, WidgetActions.approval(approval("low", allow = "no spaces allowed"), allow = true, now = now))
    }

    // MARK: privacy

    @Test fun `the report holds no command text and the lock summary leaks no names`() {
        val secret = listOf(WidgetBot("s1", "Zephyrine", "needsInput", lastAt = 5), WidgetBot("s2", "Quixotica", "working", lastAt = 9))
        val withApproval = busy.copy(bots = secret, approvals = listOf(approval("high")))
        val stored = WireJson.encodeToString(withApproval.approvals)
        for (word in listOf("command", "title", "text", "rm -rf")) assertFalse(word, word in stored)
        assertEquals(setOf("botId", "entryId", "allowOptionId", "denyOptionId", "risk", "at"), WireJson.parseToJsonElement(stored).jsonArray.single().jsonObject.keys)
        val summary = lockScreenSummary(withApproval, now).toString()
        for (bot in withApproval.bots) assertFalse("lock summary leaked ${bot.name}", bot.name in summary)
        assertFalse("Studio" in summary)
        assertNull(lockScreenSummary(withApproval, now).leadBotName)
    }

    @Test fun `an older report without the new fields still decodes`() {
        val old = """{"scope":"local","computerId":"$computer","bots":[{"id":"a","name":"A","status":"idle"}],"botsAt":5}"""
        val feed = WireJson.decodeFromString<WidgetFeed>(old)
        assertTrue(feed.approvals.isEmpty())
        assertEquals(0L, feed.offlineSince)
    }

    // MARK: where a tap goes

    @Test fun `team and approvals links name a computer and an account`() {
        val team = PageDestination.parse("codync://team?scope=local&computer=$computer", "codync")
        assertEquals(PageDestination(WidgetPage.Team, "local", computer), team)
        assertEquals(WidgetPage.Approvals, PageDestination.parse("codync://approvals?scope=local&computer=$computer", "codync").page)
        for (bad in listOf("codync://team", "codync://team?scope=local", "codync://team/x?scope=local&computer=$computer",
            "other://team?scope=local&computer=$computer", "codync://elsewhere?scope=local&computer=$computer",
            "codync://team?scope=local&computer=short")) {
            assertThrows(bad, Exception::class.java) { PageDestination.parse(bad, "codync") }
        }
    }
}
