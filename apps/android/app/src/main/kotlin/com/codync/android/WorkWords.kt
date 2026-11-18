package com.codync.android

import com.codync.android.core.Bot
import com.codync.android.core.SourceKind
import com.codync.android.core.WorkItem
import com.codync.android.core.WorkStatus
import com.codync.android.core.WaitKind
import com.codync.android.core.fill

// What the capsule, the roster and the timeline's rows say about a bot's work, in the app's language ([tr]). A title
// and a result are the bot's own words and stay as written; an activity is the host's interface phrase.

private fun counted(text: String, count: Int, tr: (String) -> String) = fill(tr(text), "count" to "$count")

/** `2 working · 3 waiting`, a part left out at none; empty with neither. */
internal fun workCounts(work: List<WorkItem>, tr: (String) -> String): String {
    val working = work.count { it.status == WorkStatus.Working }
    val waiting = work.count { it.status == WorkStatus.Waiting }
    return listOfNotNull(counted("{count} working", working, tr).takeIf { working > 0 }, counted("{count} waiting", waiting, tr).takeIf { waiting > 0 })
        .joinToString(" · ")
}

/** What the capsule says after `In the background ·`. */
internal data class CapsuleLabel(
    /** Its mark, as an item's: one needs the user, else one works, else they all wait. */
    val mark: WorkStatus,
    val text: String,
    /** When the one working item started: the capsule's time. Null when it counts items instead. */
    val since: Long?,
    /** The one item a task has, which the capsule's Stop stops; null with none or several. */
    val stops: WorkItem?,
)

/** The capsule while any item is live or waiting; null otherwise. */
internal fun capsuleLabel(work: List<WorkItem>, tr: (String) -> String): CapsuleLabel? {
    val live = work.filter { it.status.live }
    val waiting = work.count { it.status == WorkStatus.Waiting }
    if (live.isEmpty() && waiting == 0) return null
    val stops = live.singleOrNull()
    return when {
        live.any { it.status == WorkStatus.NeedsYou } ->
            CapsuleLabel(WorkStatus.NeedsYou, listOf(tr("Needs you"), workCounts(work, tr)).filter(String::isNotEmpty).joinToString(" · "), null, stops)
        stops != null && waiting == 0 ->
            CapsuleLabel(WorkStatus.Working, if (stops.activity.isEmpty()) stops.title else tr(stops.activity), stops.startedAt, stops)
        else -> CapsuleLabel(if (live.isEmpty()) WorkStatus.Waiting else WorkStatus.Working, workCounts(work, tr), null, stops)
    }
}

/** How a roster row's line reads: what needs the user, something going on (the moving mark), or plain words. */
internal enum class BotLineKind { NeedsYou, Working, Waiting }

internal data class BotLine(val kind: BotLineKind, val text: String)

/**
 * A roster row's line while something goes on, the most pressing thing first (iOS `Bot.rosterLine`): the chat or one
 * of its items needs the user, what its chat does, how many items work and wait (either part alone when the other is none). Null when the row reads as
 * usual (its last message).
 */
internal fun botLine(bot: Bot, tr: (String) -> String): BotLine? {
    val needsInput = bot.status == "needsInput"
    val working = bot.work.count { it.status == WorkStatus.Working }
    val waiting = bot.work.count { it.status == WorkStatus.Waiting }
    return when {
        needsInput -> BotLine(BotLineKind.NeedsYou, tr(bot.activity.ifEmpty { "Needs approval" }))
        bot.work.any { it.status == WorkStatus.NeedsYou } -> BotLine(BotLineKind.NeedsYou, tr(WorkStatus.NeedsYou.title))
        bot.status == "working" -> BotLine(BotLineKind.Working, tr(bot.activity.ifEmpty { "Working…" }))
        working > 0 -> BotLine(BotLineKind.Working, workCounts(bot.work, tr))
        waiting > 0 -> BotLine(BotLineKind.Waiting, workCounts(bot.work, tr))
        else -> null
    }
}

/**
 * A bot's line outside the chat (the ongoing notification), in the roster row's order: `Needs you` when an item waits
 * on the user, else the chat's own activity, else how many items work and wait; empty when there is none to say (its
 * status then).
 */
internal fun outsideLine(bot: Bot, tr: (String) -> String): String = when {
    bot.work.any { it.status == WorkStatus.NeedsYou } -> tr("Needs you")
    bot.isWorking && bot.activity.isNotEmpty() -> tr(bot.activity)
    else -> workCounts(bot.work, tr)
}

/** Why a waiting item waits. */
internal fun waitText(item: WorkItem, tr: (String) -> String): String {
    if (item.cutIn) return tr("Was cut in on; starts again")
    val wait = item.wait ?: return tr(WorkStatus.Waiting.title)
    return when (wait.kind) {
        WaitKind.Next -> tr("Next up")
        WaitKind.Behind -> counted("{count} ahead", wait.ahead, tr)
        WaitKind.After -> tr("Follows what it works on")
        WaitKind.Usage -> tr("Near the usage limit: one at a time")
    }
}

/**
 * Why a thread that takes no message has no composer: a routine's run and another bot's request have an owner already.
 * Null for the bot's own item (it takes messages, or has no thread to write to yet): nothing to say.
 */
internal fun readOnlyLine(item: WorkItem): String? = when (item.source?.kind) {
    SourceKind.Routine -> "Read only: a routine's run"
    SourceKind.Request -> "Read only: another bot's request"
    null -> null
}

/** An item's second line: why it waits, what its task does, or how it ended (a failed or stopped one says why). */
internal fun itemLine(item: WorkItem, tr: (String) -> String): String = when (item.status) {
    WorkStatus.Waiting -> waitText(item, tr)
    WorkStatus.Working -> tr(item.activity.ifEmpty { item.status.title })
    WorkStatus.Failed, WorkStatus.Stopped -> item.result.ifEmpty { tr(item.status.title) }
    WorkStatus.NeedsYou, WorkStatus.Done -> tr(item.status.title)
}
