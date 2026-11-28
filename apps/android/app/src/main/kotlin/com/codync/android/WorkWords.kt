package com.codync.android

import com.codync.android.core.Bot
import com.codync.android.core.SourceKind
import com.codync.android.core.WorkTask
import com.codync.android.core.WorkStatus
import com.codync.android.core.WaitKind
import com.codync.android.core.fill

// What the capsule, the roster and the timeline's rows say about a bot's work, in the app's language ([tr]). A title
// and a result are the bot's own words and stay as written; an activity is the host's interface phrase.

private fun counted(text: String, count: Int, tr: (String) -> String) = fill(tr(text), "count" to "$count")

/** `2 working · 3 waiting`, a part left out at none; empty with neither. */
internal fun workCounts(work: List<WorkTask>, tr: (String) -> String): String {
    val working = work.count { it.status == WorkStatus.Working }
    val waiting = work.count { it.status == WorkStatus.Waiting }
    return listOfNotNull(counted("{count} working", working, tr).takeIf { working > 0 }, counted("{count} waiting", waiting, tr).takeIf { waiting > 0 })
        .joinToString(" · ")
}

/** What the capsule says after `Work ·`. */
internal data class CapsuleLabel(
    /** Its mark, as a task's: one needs the user, else one works, else they all wait. */
    val mark: WorkStatus,
    val text: String,
    /** When the one working task started: the capsule's time. Null when it counts tasks instead. */
    val since: Long?,
    /** The one task a thread has, which the capsule's Stop stops; null with none or several. */
    val stops: WorkTask?,
)

/** The capsule while any task is live or waiting; null otherwise. */
internal fun capsuleLabel(work: List<WorkTask>, tr: (String) -> String): CapsuleLabel? {
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

/** What the capsule over a thread's composer says after `Background ·`, and since when (the oldest piece began). */
internal data class BackgroundLabel(val text: String, val since: Long)

/** The capsule while [task]'s thread's agent runs work in the background: what the one piece does, or the names of several; null while nothing runs there. */
internal fun backgroundLabel(task: WorkTask): BackgroundLabel? {
    val first = task.background.firstOrNull() ?: return null
    val text = if (task.background.size == 1) listOfNotNull(first.name.takeIf(String::isNotEmpty), first.activity).joinToString(" · ")
        else task.background.joinToString(", ") { it.name }
    return BackgroundLabel(text, first.startedAt)
}

/** How a roster row's line reads: what needs the user, something going on (the moving mark), or plain words. */
internal enum class BotLineKind { NeedsYou, Working, Waiting }

internal data class BotLine(val kind: BotLineKind, val text: String)

/**
 * A roster row's line while something goes on, the most pressing thing first (iOS `Bot.rosterLine`): the chat or one
 * of its tasks needs the user, what its chat does, how many tasks work and wait (either part alone when the other is none). Null when the row reads as
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

/** Where a bot stands when the roster is sorted by status (desktop `rosterRank`): needs you, working, waiting work, the rest. */
internal fun statusRank(bot: Bot): Int = when {
    bot.status == "needsInput" || bot.work.any { it.status == WorkStatus.NeedsYou } -> 0
    bot.status == "working" -> 1
    bot.work.any { it.status == WorkStatus.Working || it.status == WorkStatus.Waiting } -> 2
    else -> 3
}

/** [roster] sorted by [statusRank], keeping its order within each rank. */
internal fun sortedByStatus(roster: List<Bot>): List<Bot> = roster.sortedBy(::statusRank)

/**
 * A bot's line outside the chat (the ongoing notification), in the roster row's order: `Needs you` when a task waits
 * on the user, else the chat's own activity, else how many tasks work and wait; empty when there is none to say (its
 * status then).
 */
internal fun outsideLine(bot: Bot, tr: (String) -> String): String = when {
    bot.work.any { it.status == WorkStatus.NeedsYou } -> tr("Needs you")
    bot.isWorking && bot.activity.isNotEmpty() -> tr(bot.activity)
    else -> workCounts(bot.work, tr)
}

/** Which outside app started it, under the thread's title; null for the user's own. */
internal fun startedByLine(task: WorkTask, tr: (String) -> String): String? =
    task.startedBy?.label?.takeIf { it.isNotEmpty() }?.let { tr("Started by {name} (MCP)").replace("{name}", it) }

/** Why a waiting task waits. */
internal fun waitText(task: WorkTask, tr: (String) -> String): String {
    if (task.cutIn) return tr("Was cut in on; starts again")
    val wait = task.wait ?: return tr(WorkStatus.Waiting.title)
    return when (wait.kind) {
        WaitKind.Next -> tr("Next up")
        WaitKind.Behind -> counted("{count} ahead", wait.ahead, tr)
        WaitKind.After -> tr("Follows what it works on")
        WaitKind.Usage -> tr("Near the usage limit: one at a time")
    }
}

/**
 * Why a thread that takes no message has no composer: a routine's run and another bot's request have an owner already.
 * Null for the bot's own task (it takes messages, or has no thread to write to yet): nothing to say.
 */
internal fun readOnlyLine(task: WorkTask): String? = when (task.source?.kind) {
    SourceKind.Routine -> "Read only: a routine's run"
    SourceKind.Request -> "Read only: another bot's request"
    null -> null
}

/** A task's second line: why it waits, what its thread does, or how it ended (a failed or stopped one says why). */
internal fun taskLine(task: WorkTask, tr: (String) -> String): String = when (task.status) {
    WorkStatus.Waiting -> waitText(task, tr)
    WorkStatus.Working -> tr(task.activity.ifEmpty { task.status.title })
    WorkStatus.Failed, WorkStatus.Stopped -> task.result.ifEmpty { tr(task.status.title) }
    WorkStatus.NeedsYou, WorkStatus.Done -> tr(task.status.title)
}
