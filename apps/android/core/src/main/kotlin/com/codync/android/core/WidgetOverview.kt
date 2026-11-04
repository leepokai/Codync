package com.codync.android.core

import kotlinx.serialization.Serializable
import java.net.URI

// The "Codync" overview widget (`WidgetKind.Overview`): which sections it shows at each size, what it says about how old its numbers
// are, and what a tap on it may do. All of it is plain data and rules, so the views only draw (docs/guides/android-widgets-v2.md).

enum class WidgetSection { Status, Attention, Usage, Team, Recent }
enum class WidgetTheme { System, Light, Dark }
enum class WidgetDensity { Comfortable, Compact }

/** What the person chose when placing one overview widget: sections in order, whose bots, and the look. */
@Serializable data class OverviewConfig(
    /** In display priority: the first ones with something to show are the ones a small widget keeps. */
    val sections: List<WidgetSection> = DEFAULT_SECTIONS,
    /** Null: the computer the app uses. */
    val computerId: String? = null,
    /** Empty: every bot. */
    val botIds: List<String> = emptyList(),
    val theme: WidgetTheme = WidgetTheme.System,
    val density: WidgetDensity = WidgetDensity.Comfortable,
) {
    /** At least one section, none twice, a bounded bot list. */
    fun normalized(): OverviewConfig = copy(sections = sections.distinct().ifEmpty { DEFAULT_SECTIONS }, botIds = botIds.distinct().take(MAX_BOTS))

    fun encode(): String = WireJson.encodeToString(normalized())

    companion object {
        val DEFAULT_SECTIONS = listOf(WidgetSection.Status, WidgetSection.Attention, WidgetSection.Team, WidgetSection.Usage, WidgetSection.Recent)
        const val MAX_BOTS = 32

        /** A damaged or unknown value falls back to the defaults: a widget never fails to draw over its settings. */
        fun decode(text: String?): OverviewConfig = text?.let { runCatching { WireJson.decodeFromString<OverviewConfig>(it).normalized() }.getOrNull() }
            ?: OverviewConfig()
    }
}

enum class WidgetSizeClass { Small, Medium, Large, ExtraLarge }

/** The layout for a widget of `widthDp` × `heightDp`: the thresholds are the sizes the widget declares for the launcher. */
fun widgetSizeClass(widthDp: Float, heightDp: Float): WidgetSizeClass = when {
    widthDp >= 380f && heightDp >= 230f -> WidgetSizeClass.ExtraLarge
    widthDp >= 230f && heightDp >= 230f -> WidgetSizeClass.Large
    widthDp >= 230f || heightDp >= 230f -> WidgetSizeClass.Medium
    else -> WidgetSizeClass.Small
}

/** How much to trust the numbers: fresh, a while old (say how old), too old or unreachable (don't show them as current). */
enum class OverviewState { NoComputer, NoData, Fresh, Aging, Stale, Offline }

const val OVERVIEW_AGING_MS: Long = 15L * 60_000L

/** The feed as this widget sees it: its computer's, and only the bots it was set to follow. */
fun scopedFeed(feed: WidgetFeed, config: OverviewConfig): WidgetFeed {
    if (config.botIds.isEmpty()) return feed
    val keep = config.botIds.toSet()
    return feed.copy(bots = feed.bots.filter { it.id in keep }, approvals = feed.approvals.filter { it.botId in keep })
}

fun overviewState(feed: WidgetFeed, config: OverviewConfig, now: Long = System.currentTimeMillis(),
    staleMillis: Long = DEFAULT_LOCK_WIDGET_STALE_MS): OverviewState {
    if (feed.computerId.isNullOrBlank() || config.computerId != null && config.computerId != feed.computerId) return OverviewState.NoComputer
    if (feed.botsAt <= 0L) return OverviewState.NoData
    if (feed.offlineSince > 0L) return OverviewState.Offline
    val age = maxOf(0L, now - feed.botsAt)
    return when { age > staleMillis -> OverviewState.Stale; age > OVERVIEW_AGING_MS -> OverviewState.Aging; else -> OverviewState.Fresh }
}

/** Whole minutes since the report, or null when there is none. */
fun reportAgeMinutes(feed: WidgetFeed, now: Long = System.currentTimeMillis()): Long? =
    feed.botsAt.takeIf { it > 0 }?.let { maxOf(0L, now - it) / 60_000L }

/** A section with how many rows of it fit. */
data class PlannedSection(val section: WidgetSection, val rows: Int)

/** Sections in one or two columns. */
data class OverviewPlan(val state: OverviewState, val columns: List<List<PlannedSection>>) {
    val sections: List<PlannedSection> get() = columns.flatten()
}

private fun available(section: WidgetSection, feed: WidgetFeed): Int = when (section) {
    WidgetSection.Status -> 1
    WidgetSection.Attention -> feed.bots.count { it.status == "needsInput" || it.status == "error" }
    WidgetSection.Usage -> usageLimits(feed.usage).size
    WidgetSection.Team -> feed.bots.size
    WidgetSection.Recent -> feed.bots.count { it.lastAt > 0 }
}

/** The most rows a section takes on one column, before the size's budget. */
private fun cap(section: WidgetSection): Int = when (section) {
    WidgetSection.Status -> 1; WidgetSection.Attention -> 3; WidgetSection.Usage -> 3; WidgetSection.Team -> 1; WidgetSection.Recent -> 1
}

/**
 * Which sections a widget of this size draws, in the person's order, skipping the ones with nothing to say. Small keeps the status;
 * medium adds the first other section with something; large fills a column within a row budget; extra large spreads over two. Numbers
 * that are too old or unreachable show the status alone, so they aren't mistaken for current ones.
 */
fun overviewPlan(feed: WidgetFeed, config: OverviewConfig, size: WidgetSizeClass, now: Long = System.currentTimeMillis()): OverviewPlan {
    val state = overviewState(feed, config, now)
    val status = listOf(PlannedSection(WidgetSection.Status, 1))
    if (state != OverviewState.Fresh && state != OverviewState.Aging) return OverviewPlan(state, listOf(status))
    val scoped = scopedFeed(feed, config)
    val order = config.normalized().sections
    val extra = if (config.density == WidgetDensity.Compact) 1 else 0
    val others = order.filter { it != WidgetSection.Status && available(it, scoped) > 0 }
    val statusFirst = WidgetSection.Status in order
    fun planned(section: WidgetSection, budget: Int): PlannedSection? {
        val rows = minOf(cap(section), available(section, scoped), budget)
        return rows.takeIf { it > 0 }?.let { PlannedSection(section, it) }
    }
    fun column(sections: List<WidgetSection>, budget: Int, withStatus: Boolean): List<PlannedSection> {
        val result = mutableListOf<PlannedSection>()
        var left = budget
        if (withStatus) result += status
        for (section in sections) {
            val next = planned(section, left) ?: continue
            result += next; left -= next.rows
            if (left <= 0) break
        }
        return result
    }
    val columns = when (size) {
        WidgetSizeClass.Small -> listOf(if (statusFirst || others.isEmpty()) status else listOfNotNull(planned(others.first(), 1)))
        WidgetSizeClass.Medium -> listOf((if (statusFirst) status else emptyList()) + listOfNotNull(others.firstOrNull()?.let { planned(it, 2 + extra) }))
        WidgetSizeClass.Large -> listOf(column(others, 6 + extra * 2, statusFirst))
        WidgetSizeClass.ExtraLarge -> {
            // Two columns: the first takes the sections that need you most, the second the rest, each within a large budget.
            val left = column(others.take((others.size + 1) / 2), 5 + extra * 2, statusFirst)
            val right = column(others.drop((others.size + 1) / 2), 6 + extra * 2, false)
            listOf(left, right)
        }
    }
    // Never nothing: a widget whose chosen sections all have nothing to say still shows the status.
    return OverviewPlan(state, columns.filter { it.isNotEmpty() }.ifEmpty { listOf(status) })
}

// MARK: acting from the widget

/** What a tap on Allow or Deny may do. */
sealed interface ApprovalPlan {
    /** Answer the request with this option, through the same durable work a notification action uses. */
    data class Send(val optionId: String) : ApprovalPlan
    /** Not from here: open the app, which asks for the fingerprint when the request changes the computer. */
    data object OpenApp : ApprovalPlan
}

/**
 * What the widget may resolve without opening the app. Denying is always allowed. Allowing is only for requests the report marked
 * low risk (reading, searching, fetching, thinking) and only while the report is recent: a command or a file change, or anything
 * of a kind the widget doesn't know, is never allowed from here (`Approvals.isSensitive` is the app's own rule for the thumb).
 */
object WidgetActions {
    val LOW_RISK_KINDS = setOf("read", "search", "fetch", "think")
    const val ALLOW_MAX_AGE_MS: Long = 2L * 60_000L

    /** The risk the report records for a request of this tool kind; unknown kinds are high. */
    fun riskOf(toolKind: String?): String = if (toolKind in LOW_RISK_KINDS) "low" else "high"

    fun approval(approval: WidgetApproval?, allow: Boolean, now: Long = System.currentTimeMillis()): ApprovalPlan {
        if (approval == null) return ApprovalPlan.OpenApp
        if (!NotificationActionPlan.validEntry(approval.entryId)) return ApprovalPlan.OpenApp
        val option = if (allow) approval.allowOptionId else approval.denyOptionId
        if (option == null || !NotificationActionPlan.validOption(option)) return ApprovalPlan.OpenApp
        if (!allow) return ApprovalPlan.Send(option)
        val recent = approval.at > 0 && now - approval.at in 0..ALLOW_MAX_AGE_MS
        return if (approval.risk == "low" && recent) ApprovalPlan.Send(option) else ApprovalPlan.OpenApp
    }
}

// MARK: where a tap goes

/** The pages a widget opens besides one bot's chat or usage: `codync://team?…` and `codync://approvals?…` (both land on the bot list). */
enum class WidgetPage(val host: String) { Team("team"), Approvals("approvals") }

data class PageDestination(val page: WidgetPage, val scope: String, val computerId: String) {
    companion object {
        fun parse(value: String, scheme: String): PageDestination {
            val uri = URI(value)
            val page = WidgetPage.entries.first { it.host == uri.rawAuthority }
            require(uri.scheme == scheme && uri.fragment == null && uri.path.orEmpty() in setOf("", "/"))
            val scoped = BotDestination.parse("$scheme://bot/page?" + uri.rawQuery.orEmpty(), scheme)
            return PageDestination(page, scoped.scope, scoped.computerId)
        }
    }
}
