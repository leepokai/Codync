package com.codync.android.core

import kotlinx.serialization.Serializable

/** Private summaries only: widget storage never contains chat text or credentials. */
@Serializable data class WidgetBot(val id: String, val name: String, val status: String,
    val unread: Int = 0, val avatarColor: String = "blue", val avatarShape: String = "blob", val customIcon: CustomIcon? = null,
    /** The computer it lives on: bot IDs are only unique per computer. */
    val computerId: String = "",
    /** Its last activity, for the Bots tile's tie-break. */
    val lastAt: Long = 0) {
    /** The bot across every computer, as the Team widget's picks keep it (iOS re-keys `<computerId>/<botId>`). */
    val key: String get() = if (computerId.isEmpty()) id else "$computerId/$id"
}

/** Every computer's bots (the Bots and Team widgets) and one computer's usage: the last active one (iOS `lastComputerId`). */
@Serializable data class WidgetFeed(val scope: String = "local", val computerId: String? = null,
    val computerName: String = "", val bots: List<WidgetBot> = emptyList(), val botsAt: Long = 0,
    val usage: UsageReport = UsageReport(), val usageAt: Long = 0)

enum class WidgetKind { Provider, Usage, Bots, Team }

/** A limit in the Usage limits widget, named with its provider. */
data class ProviderLimit(val provider: String, val window: UsageWindow)

/** Every provider's limits for the Usage limits widget: the tightest first (its headline), then the rest in report order. */
fun usageLimits(report: UsageReport): List<ProviderLimit> {
    val all = report.providers.flatMap { provider -> provider.limits.map { ProviderLimit(provider.name, it) } }
    val top = all.withIndex().maxByOrNull { it.value.window.reportedPercent ?: -1.0 } ?: return emptyList()
    return listOf(top.value) + all.filterIndexed { index, _ -> index != top.index }
}

/** The Bots tile's bot: waiting for you, then working, then the most recent (iOS `mostUrgent`); none while every bot is quiet. */
fun mostUrgentBot(bots: List<WidgetBot>): WidgetBot? = bots.filter { it.status == "needsInput" || it.status == "working" }
    .minWithOrNull(compareBy<WidgetBot> { widgetRank(it.status) }.thenByDescending { it.lastAt })

/**
 * The Team widget's four spots: the picked bots first (a deleted pick falls back), then the feed's order (who needs you).
 * A pick saved before several computers is a bare bot ID: the first bot with that ID.
 */
fun teamSlots(bots: List<WidgetBot>, picks: List<String?>): List<WidgetBot?> {
    val picked = List(4) { i -> picks.getOrNull(i)?.let { key -> bots.firstOrNull { it.key == key } ?: bots.firstOrNull { '/' !in key && it.id == key } } }
    val queue = ArrayDeque(bots.filter { bot -> picked.none { it?.key == bot.key } })
    return picked.map { it ?: queue.removeFirstOrNull() }
}

/** Who needs you comes first in the widgets: waiting for you, working, failed, then the rest. */
fun widgetRank(status: String): Int = when (status) { "needsInput" -> 0; "working" -> 1; "error" -> 2; else -> 3 }

/** Every computer's bots, [bots] by computer ID, who needs you first. */
fun widgetBots(bots: Map<String, List<Bot>>): List<WidgetBot> = bots.flatMap { (computer, list) -> list.map { computer to it } }
    .filter { (_, bot) -> !bot.hidden && !bot.deleted }
    .sortedWith(compareBy<Pair<String, Bot>> { (_, bot) -> widgetRank(bot.status) }
        .thenByDescending { it.second.pinned }.thenByDescending { it.second.lastAt }.thenBy { it.second.name })
    .take(256).map { (computer, it) -> WidgetBot(it.id, it.name.take(256), it.status, it.unread, it.avatarColor, it.avatarShape, it.customIcon, computer, it.lastAt) }

/**
 * The feed after reading [computers] (each one's ID and bots; null when it couldn't be read, so the bots it showed stay),
 * who needs you first. A computer no longer listed takes its bots along; [botsAt] moves to [now] only when every one was [fresh].
 */
fun WidgetFeed.readingBots(computers: List<Pair<String, List<Bot>?>>, fresh: Boolean, now: Long): WidgetFeed {
    val kept = bots.filter { bot -> computers.any { (id, read) -> read == null && id == bot.computerId } }
    val read = widgetBots(computers.mapNotNull { (id, read) -> read?.let { id to it } }.toMap())
    return copy(bots = (read + kept).sortedBy { widgetRank(it.status) }.take(256), botsAt = if (computers.isNotEmpty() && fresh) now else botsAt)
}
