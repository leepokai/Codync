package com.codync.android.core

import kotlinx.serialization.Serializable

/** Private summaries only: widget storage never contains chat text or credentials. */
@Serializable data class WidgetBot(val id: String, val name: String, val status: String,
    val unread: Int = 0, val avatarColor: String = "blue", val avatarShape: String = "blob",
    /** When the bot last did something, to break ties for the Bots tile. */
    val lastAt: Long = 0, val avatarLook: String? = null)

@Serializable data class WidgetFeed(val scope: String = "local", val computerId: String? = null,
    val computerName: String = "", val bots: List<WidgetBot> = emptyList(), val botsAt: Long = 0,
    val usage: UsageReport = UsageReport(), val usageAt: Long = 0)

enum class WidgetKind { Provider, Usage, Bots, Team, LockSummary }

/** Freshness state for lock screen summary (`plan-widget-bloqueo-android.md` Phase 3). */
enum class WidgetFreshness { NoComputer, NoData, Fresh, Stale }

/**
 * Privacy-first lock screen summary (`plan-widget-bloqueo-android.md` Phases 2 & 3):
 * omits bot names, messages, and computer details by default (`includeDetails = false`).
 */
data class LockScreenSummary(
    val freshness: WidgetFreshness,
    val workingCount: Int,
    val attentionCount: Int,
    val totalBots: Int,
    val botsAt: Long,
    val ageMinutes: Long?,
    val computerName: String? = null,
    val leadBotName: String? = null,
)

const val DEFAULT_LOCK_WIDGET_STALE_MS: Long = 60L * 60_000L

fun lockScreenSummary(
    feed: WidgetFeed,
    nowMillis: Long = System.currentTimeMillis(),
    staleThresholdMillis: Long = DEFAULT_LOCK_WIDGET_STALE_MS,
    includeDetails: Boolean = false,
): LockScreenSummary {
    if (feed.computerId.isNullOrBlank()) {
        return LockScreenSummary(
            freshness = WidgetFreshness.NoComputer,
            workingCount = 0,
            attentionCount = 0,
            totalBots = 0,
            botsAt = 0L,
            ageMinutes = null,
        )
    }
    if (feed.botsAt <= 0L) {
        return LockScreenSummary(
            freshness = WidgetFreshness.NoData,
            workingCount = 0,
            attentionCount = 0,
            totalBots = feed.bots.size,
            botsAt = 0L,
            ageMinutes = null,
            computerName = if (includeDetails) feed.computerName.ifBlank { null } else null,
        )
    }
    val elapsedMs = maxOf(0L, nowMillis - feed.botsAt)
    val freshness = if (elapsedMs > staleThresholdMillis) WidgetFreshness.Stale else WidgetFreshness.Fresh
    val working = feed.bots.count { it.status == "working" }
    val attention = feed.bots.count { it.status == "needsInput" || it.status == "error" }
    val lead = if (includeDetails) mostUrgentBot(feed.bots)?.name else null
    return LockScreenSummary(
        freshness = freshness,
        workingCount = working,
        attentionCount = attention,
        totalBots = feed.bots.size,
        botsAt = feed.botsAt,
        ageMinutes = elapsedMs / 60_000L,
        computerName = if (includeDetails) feed.computerName.ifBlank { null } else null,
        leadBotName = lead,
    )
}

/** A limit in the Usage limits widget, named with its provider. */
data class ProviderLimit(val provider: String, val window: UsageWindow)

/** Every provider's limits for the Usage limits widget: the tightest first (its headline), then the rest in report order. */
fun usageLimits(report: UsageReport): List<ProviderLimit> {
    val all = report.providers.flatMap { provider -> provider.limits.map { ProviderLimit(provider.name, it) } }
    val top = all.withIndex().maxByOrNull { it.value.window.reportedPercent ?: -1.0 } ?: return emptyList()
    return listOf(top.value) + all.filterIndexed { index, _ -> index != top.index }
}

/** The Bots tile's bot, as the iPhone's control picks it: one that needs you, else one working; the most recent of those. */
fun mostUrgentBot(bots: List<WidgetBot>): WidgetBot? = bots.filter { it.status == "needsInput" || it.status == "working" }
    .minWithOrNull(compareBy<WidgetBot> { if (it.status == "needsInput") 0 else 1 }.thenByDescending { it.lastAt })

/** The Team widget's four spots: the picked bots first (a deleted pick falls back), then the feed's order (who needs you). */
fun teamSlots(bots: List<WidgetBot>, picks: List<String?>): List<WidgetBot?> {
    val picked = List(4) { i -> picks.getOrNull(i)?.let { id -> bots.firstOrNull { it.id == id } } }
    val queue = ArrayDeque(bots.filter { bot -> picked.none { it?.id == bot.id } })
    return picked.map { it ?: queue.removeFirstOrNull() }
}

fun widgetBots(bots: List<Bot>): List<WidgetBot> = bots.filter { !it.hidden && !it.deleted }
    .sortedWith(compareBy<Bot> { when (it.status) { "needsInput" -> 0; "working" -> 1; "error" -> 2; else -> 3 } }
        .thenByDescending { it.pinned }.thenByDescending { it.lastAt }.thenBy { it.name })
    .take(256).map { WidgetBot(it.id, it.name.take(256), it.status, it.unread, it.avatarColor, it.avatarShape, it.lastAt, it.avatarLook) }
