package com.codync.android.core

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.builtins.nullable
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.doubleOrNull
import kotlin.math.roundToInt

// The event box: what a bot works on away from its chat is an item in its box, and a bounded number of its agents
// take items from it. A task is the agent session working an item (`Tasks.kt` reads its transcript).

/** The six words for work, the same on the wire and in every client. */
@Serializable
enum class WorkStatus(val title: String) {
    @SerialName("waiting") Waiting("Waiting"), @SerialName("working") Working("Working"), @SerialName("needsYou") NeedsYou("Needs you"),
    @SerialName("done") Done("Done"), @SerialName("failed") Failed("Failed"), @SerialName("stopped") Stopped("Stopped");

    /** A task has it: working, or waiting on the user's approval. */
    val live: Boolean get() = this == Working || this == NeedsYou
    val ended: Boolean get() = this == Done || this == Failed || this == Stopped
    /** Still in the bot's hands: a task has it, or it waits for one. */
    val open: Boolean get() = live || this == Waiting
}

@Serializable
enum class WaitKind { @SerialName("next") Next, @SerialName("behind") Behind, @SerialName("after") After, @SerialName("usage") Usage }

/** Why an item waits: a place to come free, items ahead of it ([ahead]), the item it follows, or the agent's usage limit. */
@Serializable
data class WorkWait(val kind: WaitKind, val ahead: Int = 1)

@Serializable
enum class SourceKind { @SerialName("routine") Routine, @SerialName("request") Request }

/** Where an item came from when the bot didn't add it itself: a run of the routine [id], or a request from the bot
 *  [id] (null when the asking bot isn't one of this computer's). */
@Serializable
data class WorkSource(val kind: SourceKind, val id: String? = null)

/** The git worktree a task works in, when it started while another task of the bot worked in the same repository. */
@Serializable
data class Worktree(val branch: String, val path: String = "")

/** One piece of work in a bot's box (`bot.work`). */
@Serializable
data class WorkItem(
    val id: String,
    val title: String = "",
    val status: WorkStatus,
    /** What its task is doing now, in the host's words; empty until it says. */
    val activity: String = "",
    /** The task that has or had it; null for one no task has taken. */
    val task: String? = null,
    val wait: WorkWait? = null,
    /** It gave way to another item once, and starts again from its brief. */
    val cutIn: Boolean = false,
    val createdAt: Long = 0,
    val startedAt: Long? = null,
    val endedAt: Long? = null,
    /** What its task reported, or why it failed. */
    val result: String = "",
    /** Null for one the bot added. */
    val source: WorkSource? = null,
    /** Null for a task that works in the bot's own folder. */
    val worktree: Worktree? = null,
)

/** The most agents a bot may run at once. */
const val MAX_AGENTS = 6

/** How many a bot runs at once unless the user changed it: what a host that sends no `maxAgents` is counted as. */
const val DEFAULT_MAX_AGENTS = 3

/**
 * A bot's `work`, read leniently: a list this app can't read is no items, a null field is a missing one, a reason to
 * wait it can't read is none, a source it doesn't know reads as the bot's own, and an item it still can't read (no id, a status it doesn't know) is left out, never a lost bot.
 */
internal object WorkSerializer : KSerializer<List<WorkItem>> {
    private val list = ListSerializer(WorkItem.serializer())
    override val descriptor = list.descriptor
    override fun serialize(encoder: Encoder, value: List<WorkItem>) = list.serialize(encoder, value)
    override fun deserialize(decoder: Decoder): List<WorkItem> {
        val json = decoder as? JsonDecoder ?: return list.deserialize(decoder)
        return (json.decodeJsonElement() as? JsonArray).orEmpty().mapNotNull { item ->
            val fields = item.present() ?: return@mapNotNull null
            val wait = fields["wait"]?.present()?.let { runCatching { json.json.decodeFromJsonElement(WorkWait.serializer(), JsonObject(it)) }.getOrNull() }
            val source = fields["source"]?.present()?.let { runCatching { json.json.decodeFromJsonElement(WorkSource.serializer(), JsonObject(it)) }.getOrNull() }
            val worktree = fields["worktree"]?.present()?.let { runCatching { json.json.decodeFromJsonElement(Worktree.serializer(), JsonObject(it)) }.getOrNull() }
                ?.takeIf { it.branch.isNotEmpty() }
            runCatching { json.json.decodeFromJsonElement(WorkItem.serializer(), JsonObject(fields - "wait" - "source" - "worktree")) }.getOrNull()
                ?.takeIf { it.id.isNotEmpty() }?.copy(wait = wait, source = source, worktree = worktree)
        }
    }
}

/** An object's fields that have a value: a null field is a missing one. */
private fun JsonElement.present(): Map<String, JsonElement>? = (this as? JsonObject)?.filterValues { it !is JsonNull }

/**
 * How a bot stands outside its chat (`bot.outside`): it needs the user while the chat or one of its items waits on
 * them, works while the chat works or its box holds anything that isn't over, else is as its chat. What keeps the
 * ongoing notification and the widgets going while items work after the chat's turn ended.
 */
@Serializable
data class Outside(val status: String, val startedAt: Long? = null)

/** A bot's `outside`, read leniently: one this app can't read is none (the chat's own status then), never a lost bot. */
internal object OutsideSerializer : KSerializer<Outside?> {
    private val outside = Outside.serializer().nullable
    override val descriptor = outside.descriptor
    override fun serialize(encoder: Encoder, value: Outside?) = outside.serialize(encoder, value)
    override fun deserialize(decoder: Decoder): Outside? {
        val json = decoder as? JsonDecoder ?: return outside.deserialize(decoder)
        val fields = json.decodeJsonElement().present() ?: return null
        return runCatching { json.json.decodeFromJsonElement(Outside.serializer(), JsonObject(fields)) }.getOrNull()?.takeIf { it.status.isNotEmpty() }
    }
}

/** A bot's `maxAgents`, within 1 to [MAX_AGENTS]; null from a host before the box, or for a value this app can't read. */
internal object MaxAgentsSerializer : KSerializer<Int?> {
    private val number = Int.serializer().nullable
    override val descriptor = number.descriptor
    override fun serialize(encoder: Encoder, value: Int?) = number.serialize(encoder, value)
    override fun deserialize(decoder: Decoder): Int? {
        val json = decoder as? JsonDecoder ?: return number.deserialize(decoder)
        val value = (json.decodeJsonElement() as? JsonPrimitive)?.takeIf { !it.isString }?.doubleOrNull ?: return null
        return value.roundToInt().coerceIn(1, MAX_AGENTS)
    }
}

/** All bots' work in one view: the roster's bots that have a live or waiting item, in roster order ([bots]), and how
 *  many items are worked on in all ([working]: the count on the view's button). */
data class AllWork<T>(val bots: List<T>, val working: Int)

fun <T> allWork(roster: List<T>, work: (T) -> List<WorkItem>): AllWork<T> {
    val bots = roster.filter { bot -> work(bot).any { it.status.open } }
    return AllWork(bots, bots.sumOf { bot -> work(bot).count { it.status == WorkStatus.Working } })
}

/** The Details timeline's sections of items, each in the order its rows show. */
data class WorkSections(
    /** The agent sessions that run now: items that need the user, then the working ones. */
    val threads: List<WorkItem>,
    /** The most the bot runs at once: the Threads heading's `2 / 3` is how many run of this (`2 / 1` right after it was lowered). */
    val places: Int,
    /** What waits for a thread, in the order it will be taken. */
    val box: List<WorkItem>,
    /** Items that ended in the last day, newest first. */
    val recent: List<WorkItem>,
) {
    /** The bot's places not in use: the ceiling less the threads. */
    val free: Int get() = maxOf(0, places - threads.size)
}

fun workSections(work: List<WorkItem>, maxAgents: Int?): WorkSections {
    val threads = work.filter { it.status == WorkStatus.NeedsYou } + work.filter { it.status == WorkStatus.Working }
    return WorkSections(threads, places = maxAgents ?: DEFAULT_MAX_AGENTS,
        box = work.filter { it.status == WorkStatus.Waiting }, recent = work.filter { it.status.ended })
}

/** A stopped or failed item can go back to waiting: not another bot's request, which that bot asks again. A routine's runs the routine again. */
fun canRetry(item: WorkItem): Boolean = (item.status == WorkStatus.Failed || item.status == WorkStatus.Stopped) && item.source?.kind != SourceKind.Request

/** A task has it and it may give way to an item that cuts in: only work the bot added. A routine's run or another
 *  bot's request is stopped, never set aside. */
fun givesWay(item: WorkItem): Boolean = item.status.live && item.source == null

/**
 * Every place of the bot is taken, so the item first in line can only start now if a task gives way, and one may. Not
 * one that follows unfinished work or waits for usage: a freed place wouldn't start it.
 */
fun canCutIn(work: List<WorkItem>, maxAgents: Int?): Boolean {
    val held = work.firstOrNull { it.status == WorkStatus.Waiting }?.wait?.kind.let { it == WaitKind.After || it == WaitKind.Usage }
    return !held && work.any(::givesWay) && work.count { it.status.live } >= (maxAgents ?: DEFAULT_MAX_AGENTS)
}

/** What the user can do to an item from its row (iOS `WorkAction`): a phrase to look up and its symbol. */
enum class WorkAction(val title: String, val symbol: String) {
    Stop("Stop", "stop.fill"), First("First", "arrow.up.to.line"), CutIn("Cut in", "arrow.turn.right.up"), Drop("Drop", "xmark"),
    Retry("Try again", "arrow.clockwise");

    val destructive: Boolean get() = this == Stop || this == Drop
}

/** What [item]'s row offers among a bot's [work], the one that shows on the row first. */
fun workActions(item: WorkItem, work: List<WorkItem>, maxAgents: Int?): List<WorkAction> = when (item.status) {
    WorkStatus.Working, WorkStatus.NeedsYou -> listOf(WorkAction.Stop)
    WorkStatus.Waiting -> when {
        item.id != work.firstOrNull { it.status == WorkStatus.Waiting }?.id -> listOf(WorkAction.First, WorkAction.Drop)
        canCutIn(work, maxAgents) -> listOf(WorkAction.CutIn, WorkAction.Drop)
        else -> listOf(WorkAction.Drop)
    }
    WorkStatus.Failed, WorkStatus.Stopped -> if (canRetry(item)) listOf(WorkAction.Retry, WorkAction.Drop) else listOf(WorkAction.Drop)
    WorkStatus.Done -> listOf(WorkAction.Drop)
}
