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
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlin.math.roundToInt

// The event box: what a bot works on away from its chat is a task in its box, and a bounded number of its threads
// take tasks from it. A thread is where a task is worked, with its own session (`WorkThreads.kt` reads its transcript).

/** The six words for work, the same on the wire and in every client. */
@Serializable
enum class WorkStatus(val title: String) {
    @SerialName("waiting") Waiting("Waiting"), @SerialName("working") Working("Working"), @SerialName("needsYou") NeedsYou("Needs you"),
    @SerialName("done") Done("Done"), @SerialName("failed") Failed("Failed"), @SerialName("stopped") Stopped("Stopped");

    /** A thread has it: working, or waiting on the user's approval. */
    val live: Boolean get() = this == Working || this == NeedsYou
    val ended: Boolean get() = this == Done || this == Failed || this == Stopped
    /** Still in the bot's hands: a thread has it, or it waits for one. */
    val open: Boolean get() = live || this == Waiting
}

@Serializable
enum class WaitKind { @SerialName("next") Next, @SerialName("behind") Behind, @SerialName("after") After, @SerialName("usage") Usage }

/** Why a task waits: a thread to come free, tasks ahead of it ([ahead]), the task it follows, or the agent's usage limit. */
@Serializable
data class WorkWait(val kind: WaitKind, val ahead: Int = 1)

@Serializable
enum class SourceKind { @SerialName("routine") Routine, @SerialName("request") Request }

/** Where a task came from when the bot didn't add it itself: a run of the routine [id], or a request from the bot
 *  [id] (null when the asking bot isn't one of this computer's). */
@Serializable
data class WorkSource(val kind: SourceKind, val id: String? = null)

/** The git worktree a thread works in, when it started while another thread of the bot worked in the same repository. */
@Serializable
data class Worktree(val branch: String, val path: String = "")

/** A piece of work a thread's agent reports running in the background: a command, a workflow, a monitor. */
@Serializable data class BackgroundTask(
    val id: String,
    /** What the agent calls it: a workflow's name, a command's description. */
    val name: String,
    /** What it does now, when the agent says (a workflow's phase and agent). */
    val activity: String? = null,
    val startedAt: Long = 0,
)

/** One piece of work in a bot's box (`bot.work`). */
@Serializable
data class WorkTask(
    val id: String,
    val title: String = "",
    val status: WorkStatus,
    /** What its thread is doing now, in the host's words; empty until it says. */
    val activity: String = "",
    /** The thread that has or had it; null for one no thread has taken. */
    val thread: String? = null,
    val wait: WorkWait? = null,
    /** It gave way to another task once, and starts again from its brief. */
    val cutIn: Boolean = false,
    val createdAt: Long = 0,
    val startedAt: Long? = null,
    val endedAt: Long? = null,
    /** What its thread reported, or why it failed. */
    val result: String = "",
    /** Null for one the bot added. */
    val source: WorkSource? = null,
    /** Null for a thread that works in the bot's own folder. */
    val worktree: Worktree? = null,
    /** It takes the user's messages ([canTalk]): the bot's own task whose thread still exists. False from an older host. */
    val talk: Boolean = false,
    /** Its thread takes files with the user's messages too. False from a host before them, where the box has no +. */
    val talkFiles: Boolean = false,
    /** Where its thread works (a worktree's folder for one in a worktree); null for one no thread has taken, and from an older host. */
    val folder: String? = null,
    /** [folder] is the one Codync made for a thread without a bot, which reads "Own folder". False from an older host. */
    val ownFolder: Boolean = false,
    /** The agent its thread runs on (a backend id); null for the bot's own, and from an older host. */
    val agent: String? = null,
    /** It takes one of the bot's seats, as the host says (one waiting on the user gives its seat up); null from an older host ([takesSeat]). */
    val seat: Boolean? = null,
    /** The agent its thread runs on now, or ran on last (a backend id); null before a thread took it, and from an older host. */
    val runsOn: String? = null,
    /** What its thread's agent runs in the background, oldest first: the task waits on it. Empty from an older host, and for an agent that doesn't say. */
    val background: List<BackgroundTask> = emptyList(),
) {
    /** The agent its thread runs on, or will: its own, else the one it was started on, else [bot]'s (none for the box of threads without a bot). */
    fun agentOf(bot: Bot): String? = runsOn ?: agent ?: bot.backend.takeUnless { bot.plain }

    /** It takes one of the bot's seats (`maxThreads`): the host's word, else a live task. */
    val takesSeat: Boolean get() = seat ?: status.live
}

/** An ended thread from `olderThreads`: its bot ([botId], which may be the box of threads without a bot) and its task. */
data class OlderThread(val botId: String, val task: WorkTask)

/**
 * A computer's older threads (`olderThreads`, the ones past the day a bot's `work` keeps), newest first, page by page;
 * [more] while it holds older ones still.
 */
data class OlderThreads(
    val threads: List<OlderThread> = emptyList(),
    val more: Boolean = true,
    /** Ended threads that left their bot's `work` (past its last day) while connected, as they last read. */
    val kept: List<OlderThread> = emptyList(),
    /** Threads dropped here: their bot's next `work` without them doesn't keep them. */
    val dropped: Set<Pair<String, String>> = emptySet(),
) {
    /** Where the next page starts: the end of the oldest paged; null before the first. */
    val before: Long? get() = threads.mapNotNull { it.task.endedAt }.minOrNull()

    /** [botId]'s tasks among them: a kept one over its paged row. */
    fun of(botId: String): List<WorkTask> = (kept + threads).filter { it.botId == botId }.map { it.task }.distinctBy { it.id }

    /** [page] after these, without a thread already held, and whether the computer has more. */
    operator fun plus(page: OlderThreads): OlderThreads {
        val held = threads.mapTo(HashSet()) { it.botId to it.task.id }
        return copy(threads = threads + page.threads.filter { (it.botId to it.task.id) !in held }, more = page.more)
    }

    /** [botId]'s `work` went from [was] to [now]: an ended thread it no longer carries stays listed, as it last read. */
    fun keep(botId: String, was: List<WorkTask>, now: List<WorkTask>): OlderThreads {
        val carried = now.mapTo(HashSet()) { it.id }
        val left = was.filter { it.endedAt != null && it.id !in carried && (botId to it.id) !in dropped }
        if (left.isEmpty()) return this
        val ids = left.mapTo(HashSet()) { it.id }
        return copy(kept = kept.filterNot { it.botId == botId && it.task.id in ids } + left.map { OlderThread(botId, it) })
    }

    /** [taskId] of [botId] dropped (`dropWork`): out of the list, and not kept once it leaves its bot's `work`. */
    fun drop(botId: String, taskId: String): OlderThreads = copy(
        threads = threads.filterNot { it.botId == botId && it.task.id == taskId },
        kept = kept.filterNot { it.botId == botId && it.task.id == taskId },
        dropped = dropped + (botId to taskId),
    )

    companion object {
        /** A page as the computer answers it; each row is read on its own, so one this app can't read is left out alone. */
        fun decode(value: JsonObject): OlderThreads = OlderThreads(
            (value["threads"] as? JsonArray).orEmpty().mapNotNull { row ->
                val botId = ((row as? JsonObject)?.get("botId") as? JsonPrimitive)?.takeIf { it.isString }?.content ?: return@mapNotNull null
                WireJson.decodeFromJsonElement(WorkSerializer, JsonArray(listOf(row))).firstOrNull()?.let { OlderThread(botId, it) }
            },
            (value["more"] as? JsonPrimitive)?.booleanOrNull == true,
        )
    }
}

/** A bot's [work] with the [older] tasks it doesn't hold after it: the bot's own row of a task wins. */
fun mergedWork(work: List<WorkTask>, older: List<WorkTask>): List<WorkTask> {
    if (older.isEmpty()) return work
    val held = work.mapTo(HashSet()) { it.id }
    return work + older.filter { it.id !in held }
}

/** The most threads a bot may run at once. */
const val MAX_THREADS = 6

/** How many a bot runs at once unless the user changed it: what a host that sends no `maxThreads` is counted as. */
const val DEFAULT_MAX_THREADS = 3

/**
 * A bot's `work`, read leniently: a list this app can't read is no tasks, a null field is a missing one, a reason to
 * wait it can't read is none, a source it doesn't know reads as the bot's own, and a task it still can't read (no id, a status it doesn't know) is left out, never a lost bot.
 */
internal object WorkSerializer : KSerializer<List<WorkTask>> {
    private val list = ListSerializer(WorkTask.serializer())
    override val descriptor = list.descriptor
    override fun serialize(encoder: Encoder, value: List<WorkTask>) = list.serialize(encoder, value)
    override fun deserialize(decoder: Decoder): List<WorkTask> {
        val json = decoder as? JsonDecoder ?: return list.deserialize(decoder)
        return (json.decodeJsonElement() as? JsonArray).orEmpty().mapNotNull { task ->
            // Hosts before 2.51.0: goes when MIN_HOST reaches 2.51.0.
            val fields = task.present()?.oldWorkRow() ?: return@mapNotNull null
            val wait = fields["wait"]?.present()?.let { runCatching { json.json.decodeFromJsonElement(WorkWait.serializer(), JsonObject(it)) }.getOrNull() }
            val source = fields["source"]?.present()?.let { runCatching { json.json.decodeFromJsonElement(WorkSource.serializer(), JsonObject(it)) }.getOrNull() }
            val worktree = fields["worktree"]?.present()?.let { runCatching { json.json.decodeFromJsonElement(Worktree.serializer(), JsonObject(it)) }.getOrNull() }
                ?.takeIf { it.branch.isNotEmpty() }
            val background = (fields["background"] as? JsonArray).orEmpty().mapNotNull { backgroundTask(it) }
            runCatching { json.json.decodeFromJsonElement(WorkTask.serializer(), JsonObject(fields - "wait" - "source" - "worktree" - "background")) }.getOrNull()
                ?.takeIf { it.id.isNotEmpty() }?.copy(wait = wait, source = source, worktree = worktree, background = background)
        }
    }
}

/** One of a task's `background` items; one without an id is unreadable. */
private fun backgroundTask(item: JsonElement): BackgroundTask? {
    val fields = item.present() ?: return null
    fun text(key: String) = (fields[key] as? JsonPrimitive)?.takeIf { it.isString }?.content
    val id = text("id")?.takeIf { it.isNotEmpty() } ?: return null
    val startedAt = (fields["startedAt"] as? JsonPrimitive)?.doubleOrNull?.toLong() ?: 0
    return BackgroundTask(id, text("name").orEmpty(), text("activity")?.takeIf { it.isNotEmpty() }, startedAt)
}

/** An object's fields that have a value: a null field is a missing one. */
private fun JsonElement.present(): Map<String, JsonElement>? = (this as? JsonObject)?.filterValues { it !is JsonNull }

/**
 * How a bot stands outside its chat (`bot.outside`): it needs the user while the chat or one of its tasks waits on
 * them, works while the chat works or its box holds anything that isn't over, else is as its chat. What keeps the
 * ongoing notification and the widgets going while tasks work after the chat's turn ended.
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

/** A bot's `maxThreads`, within 1 to [MAX_THREADS]; null from a host before the box, or for a value this app can't read. */
internal object MaxThreadsSerializer : KSerializer<Int?> {
    private val number = Int.serializer().nullable
    override val descriptor = number.descriptor
    override fun serialize(encoder: Encoder, value: Int?) = number.serialize(encoder, value)
    override fun deserialize(decoder: Decoder): Int? {
        val json = decoder as? JsonDecoder ?: return number.deserialize(decoder)
        val value = (json.decodeJsonElement() as? JsonPrimitive)?.takeIf { !it.isString }?.doubleOrNull ?: return null
        return value.roundToInt().coerceIn(1, MAX_THREADS)
    }
}

/** How the Threads list groups the threads that don't need the user (desktop `threadsGroup`); [None] lists them all under Recent. */
enum class ThreadGrouping { None, Folder, Bot, Computer }

/** What a Threads group gathers: a folder a thread works in outside its bot's own, a routine's runs, a bot, or a computer. */
enum class ThreadGroupKind { Folder, Routine, Bot, Computer }

data class ThreadRow<T>(val of: T, val task: WorkTask)

/** The one Threads group of every thread in a folder Codync made for it ([WorkTask.ownFolder]), titled "Own folder", a phrase the view looks up. */
const val OWN_FOLDER_GROUP = "folder:own"

/** One group of the Threads list; [title] is the folder's last name, the routine's, the bot's or the computer's. */
data class ThreadGroup<T>(val key: String, val kind: ThreadGroupKind, val title: String, val rows: List<ThreadRow<T>>)

data class ThreadSections<T>(val needsYou: List<ThreadRow<T>>, val groups: List<ThreadGroup<T>>, val recent: List<ThreadRow<T>>)

/** Where a task's thread works: its folder, else its worktree's. */
val WorkTask.wherePath: String? get() = listOfNotNull(folder, worktree?.path).firstOrNull { it.isNotEmpty() }

/** When a thread last moved: it ended, else it started, else it was added. */
private val WorkTask.lastActive: Long get() = endedAt ?: startedAt ?: createdAt

/** Where a thread stands in a list sorted by status: working, waiting, the rest. */
private fun threadRank(status: WorkStatus) = when (status) { WorkStatus.Working -> 0; WorkStatus.Waiting -> 1; else -> 2 }

/**
 * Every task of [roster]'s bots as the Threads list shows it (desktop `threadSections`), newest first (by last activity with
 * [byActivity], else by when it was added; by status first with [byStatus]): what needs the user, then the groups [grouping]
 * makes (folder grouping also gathers routines' runs; a bot's and a computer's groups keep the roster's order, the others
 * come newest first), then the rest. [keyOf] tells bots apart across computers; [computerOf] names a bot's computer.
 */
fun <T> threadSections(roster: List<T>, bot: (T) -> Bot, keyOf: (T) -> String, computerOf: (T) -> String, grouping: ThreadGrouping,
    byStatus: Boolean = false, byActivity: Boolean = false): ThreadSections<T> {
    val time: (WorkTask) -> Long = if (byActivity) { task -> task.lastActive } else { task -> task.createdAt }
    val order = compareBy<ThreadRow<T>> { if (byStatus) threadRank(it.task.status) else 0 }.thenByDescending { time(it.task) }
    val rows = roster.flatMap { of -> bot(of).work.map { ThreadRow(of, it) } }
    val (needsYou, rest) = rows.partition { it.task.status == WorkStatus.NeedsYou }
    val groups = linkedMapOf<String, ThreadGroup<T>>()
    val recent = mutableListOf<ThreadRow<T>>()
    for (row in rest.sortedWith(order)) {
        val (of, task) = row
        val path = task.wherePath
        val source = task.source
        val group: ThreadGroup<T>? = when {
            grouping == ThreadGrouping.Bot -> ThreadGroup("bot:${keyOf(of)}", ThreadGroupKind.Bot, bot(of).name, emptyList())
            grouping == ThreadGrouping.Computer -> ThreadGroup("computer:${computerOf(of)}", ThreadGroupKind.Computer, computerOf(of), emptyList())
            grouping == ThreadGrouping.None -> null
            source?.kind == SourceKind.Routine -> ThreadGroup("routine:${keyOf(of)}:${source.id ?: task.title}", ThreadGroupKind.Routine, task.title, emptyList())
            task.ownFolder -> ThreadGroup(OWN_FOLDER_GROUP, ThreadGroupKind.Folder, "Own folder", emptyList())
            path != null && task.worktree == null && path != bot(of).cwd ->
                ThreadGroup("folder:$path", ThreadGroupKind.Folder, folderName(path), emptyList())
            else -> null
        }
        if (group == null) recent += row
        else groups[group.key] = (groups[group.key] ?: group).let { it.copy(rows = it.rows + row) }
    }
    val ordered = when (grouping) {
        ThreadGrouping.Bot -> roster.map { "bot:${keyOf(it)}" }.distinct().mapNotNull(groups::get)
        ThreadGrouping.Computer -> roster.map { "computer:${computerOf(it)}" }.distinct().mapNotNull(groups::get)
        else -> groups.values.toList()
    }
    return ThreadSections(needsYou.sortedWith(order), ordered, recent)
}

/**
 * What the Threads list's search finds (desktop `threadItems`): [roster]'s tasks that have a thread whose title or bot's
 * name holds [query] (the box without a bot has no name to match), newest first; all of them for an empty [query].
 */
fun <T> threadMatches(roster: List<T>, bot: (T) -> Bot, query: String): List<ThreadRow<T>> {
    val words = query.trim()
    return roster.flatMap { of -> bot(of).work.filter { it.thread != null }.map { ThreadRow(of, it) } }
        .filter { (of, task) -> task.title.contains(words, ignoreCase = true) || !bot(of).plain && bot(of).name.contains(words, ignoreCase = true) }
        .sortedByDescending { it.task.createdAt }
}

/** The Details timeline's sections of tasks, each in the order its rows show. */
data class WorkSections(
    /** The threads that run now: tasks that need the user, then the working ones. */
    val threads: List<WorkTask>,
    /** The seats taken: the Threads heading's `2` of `2 / 3` (a thread waiting on the user takes none). */
    val inUse: Int,
    /** The most the bot runs at once: the Threads heading's `3` of `2 / 3` (`2 / 1` right after it was lowered). */
    val atOnce: Int,
    /** What waits for a thread, in the order it will be taken. */
    val box: List<WorkTask>,
    /** Tasks that ended in the last day, newest first. */
    val recent: List<WorkTask>,
) {
    /** The threads the bot could still start: the ceiling less the seats taken. */
    val free: Int get() = maxOf(0, atOnce - inUse)
}

fun workSections(work: List<WorkTask>, maxThreads: Int?): WorkSections {
    val threads = work.filter { it.status == WorkStatus.NeedsYou } + work.filter { it.status == WorkStatus.Working }
    return WorkSections(threads, inUse = work.count { it.takesSeat }, atOnce = maxThreads ?: DEFAULT_MAX_THREADS,
        box = work.filter { it.status == WorkStatus.Waiting }, recent = work.filter { it.status.ended })
}

/** A stopped or failed task can go back to waiting: not another bot's request, which that bot asks again. A routine's runs the routine again. */
fun canRetry(task: WorkTask): Boolean = (task.status == WorkStatus.Failed || task.status == WorkStatus.Stopped) && task.source?.kind != SourceKind.Request

/** A thread has it and it may give way to one that cuts in: only work the bot added, in one of its seats. A routine's
 *  run or another bot's request is stopped, never set aside, and a thread the user started holds no seat to free. */
fun givesWay(task: WorkTask): Boolean = task.status.live && task.source == null && task.seat != false

/**
 * Every thread of the bot is busy, so the task first in line can only start now if a working one gives way, and one may. Not
 * one that follows unfinished work or waits for usage: a free thread wouldn't start it.
 */
fun canCutIn(work: List<WorkTask>, maxThreads: Int?): Boolean {
    val held = work.firstOrNull { it.status == WorkStatus.Waiting }?.wait?.kind.let { it == WaitKind.After || it == WaitKind.Usage }
    return !held && work.any(::givesWay) && work.count { it.takesSeat } >= (maxThreads ?: DEFAULT_MAX_THREADS)
}

/** What the user can do to a task from its row (iOS `WorkAction`): a phrase to look up and its symbol. */
enum class WorkAction(val title: String, val symbol: String) {
    Stop("Stop", "stop.fill"), First("First", "arrow.up.to.line"), CutIn("Cut in", "arrow.turn.right.up"), Drop("Drop", "xmark"),
    Retry("Try again", "arrow.clockwise");

    val destructive: Boolean get() = this == Stop || this == Drop
}

/** What [task]'s row offers among a bot's [work], the one that shows on the row first. */
fun workActions(task: WorkTask, work: List<WorkTask>, maxThreads: Int?): List<WorkAction> = when (task.status) {
    WorkStatus.Working, WorkStatus.NeedsYou -> listOf(WorkAction.Stop)
    WorkStatus.Waiting -> when {
        task.id != work.firstOrNull { it.status == WorkStatus.Waiting }?.id -> listOf(WorkAction.First, WorkAction.Drop)
        canCutIn(work, maxThreads) -> listOf(WorkAction.CutIn, WorkAction.Drop)
        else -> listOf(WorkAction.Drop)
    }
    WorkStatus.Failed, WorkStatus.Stopped -> if (canRetry(task)) listOf(WorkAction.Retry, WorkAction.Drop) else listOf(WorkAction.Drop)
    WorkStatus.Done -> listOf(WorkAction.Drop)
}

/** What a task's where line shows before its text. */
enum class WhereKind(val symbol: String) { Folder("folder"), Branch("arrow.triangle.branch"), Own("folder") }

/** Where a task's thread works, as its row and its thread's header say it: [text] after the symbol, and the full [path] a tap copies (null when the host didn't say). */
data class WhereLine(val kind: WhereKind, val text: String, val path: String?)

/** The branch of a thread in a worktree of its own, else the folder it works in, the computer's [home] as `~` ([homeRelative]); null for a task no thread has taken. */
fun whereLine(task: WorkTask, home: String): WhereLine? {
    val path = task.folder ?: task.worktree?.path?.takeIf(String::isNotEmpty)
    task.worktree?.let { return WhereLine(WhereKind.Branch, it.branch, path) }
    // The folder Codync made for it: a phrase the view looks up, not its id.
    if (task.ownFolder && task.folder != null) return WhereLine(WhereKind.Own, "Own folder", task.folder)
    return task.folder?.let { WhereLine(WhereKind.Folder, homeRelative(it, home), it) }
}

/** [path] with the computer's [home] folder as `~`, on either kind of separator; unchanged outside the home folder or without one. */
fun homeRelative(path: String, home: String): String {
    val root = home.trimEnd('/', '\\')
    if (root.isEmpty() || !path.startsWith(root)) return path
    val rest = path.substring(root.length)
    return if (rest.isEmpty() || rest[0] == '/' || rest[0] == '\\') "~$rest" else path
}

/**
 * The key of the draft the user writes to [botId]'s task [taskId] in its thread (iOS `WorkThread.draftKey`): under the
 * bot's, so it goes with the bot. `item` is what a task was called when drafts were first kept: the key stays, so a
 * draft written before 2.51.0 is still there.
 */
fun threadDraftKey(botId: String, taskId: String) = "$botId/item/$taskId"

/** Whether a bot's folder is in a git repository, as Settings > Workspace says it: a phrase to look up. */
enum class GitState(val title: String) {
    Repo("Git repository"), None("Not a git repository"), Missing("Git isn't installed on this computer")
}

/** [bot]'s [GitState]; null from a host that doesn't say (one before worktrees). A host that doesn't say whether git is installed has it. */
fun gitState(bot: Bot): GitState? = when {
    bot.threadWorktrees == null -> null
    bot.gitInstalled == false -> GitState.Missing
    bot.gitRepo -> GitState.Repo
    else -> GitState.None
}

/** A thread's transcript in two: what the host told the thread first (the task's prompt, folded on top; null when the transcript doesn't reach back to it) and the [rest], oldest first. */
data class ThreadTranscript(val told: Entry?, val rest: List<Entry>)

/** The host's own words to a thread (`from: host`), never a message of the user's. */
val Entry.isToldToThread: Boolean get() = kind == "user" && data.text("from") == "host"

/** [entries] without the messages taken back: the first thing the host told the thread, and the rest in order (what the host told it later stays in its place). */
fun threadTranscript(entries: List<Entry>): ThreadTranscript {
    val kept = entries.filterNot(Entry::isRemoved)
    val told = kept.firstOrNull(Entry::isToldToThread)
    return ThreadTranscript(told, kept.filterNot { it === told })
}

/** A message of the user's that waits in the thread: what the host told it, the chat passing words on included, never does. */
private val Entry.waitsInThread: Boolean get() = isQueued && !isToldToThread

/** While the thread works, the user's messages that wait above the composer, oldest first (iOS `ThreadTranscript.queue`).
 *  What the host told it stays quoted where it came. */
fun ThreadTranscript.queue(working: Boolean): List<Entry> = if (working) rest.filter { it.waitsInThread }.sortedBy(Entry::seq) else emptyList()

/** [ThreadTranscript.rest] without that queue. */
fun ThreadTranscript.hidingQueue(working: Boolean): List<Entry> = if (working) rest.filterNot { it.waitsInThread } else rest

/**
 * A thread's [entries] with what it ([thread]: its id) put in the bot's [chat] (a file, a voice note, an artifact, a card,
 * answered or not) where it came (`createdAt`), each kept in its order; at the same moment the
 * thread's own first.
 */
fun sharing(entries: List<Entry>, chat: List<Entry>, thread: String): List<Entry> {
    val shared = ArrayDeque(chat.filter { (it.data["thread"] as? JsonObject)?.text("id") == thread })
    val out = mutableListOf<Entry>()
    for (entry in entries) {
        while (shared.firstOrNull()?.let { it.createdAt < entry.createdAt } == true) out += shared.removeFirst()
        out += entry
    }
    return out + shared
}

/** A folder's name: its path's last part (`/` or `\` separated), or the path itself. */
fun folderName(path: String): String = path.split('/', '\\').lastOrNull { it.isNotEmpty() } ?: path

/** Where it works, as a list shows it (CodyncKit `Bot.folderName`): its default project's name; null for its own folder,
 *  which the caller names in its language; or (an older host) its project folder's name. */
val Bot.folderName: String? get() = projects.firstOrNull()?.let(::folderName) ?: if (managedWorkspace) null else folderName(cwd)
