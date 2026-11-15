package com.codync.android.core

import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject

/**
 * Long work a bot does away from its chat (iOS `BotTask`): one of a bot's `tasks` while it goes, or the task
 * `taskEntries` answers with (which may have ended: [endedAt]).
 */
@Serializable
data class BotTask(
    val id: String,
    val title: String = "",
    val status: TaskStatus = TaskStatus.Working,
    /** Its current step, in the host's words; empty before it has one. */
    val activity: String = "",
    val startedAt: Long = 0,
    val endedAt: Long? = null,
) {
    /** What it is doing, in a few words: a phrase to look up ([phrase]) or its own step or title as written. */
    val what: TaskWhat get() = when (status) {
        TaskStatus.NeedsInput -> TaskWhat("Needs you", phrase = true)
        TaskStatus.Starting -> TaskWhat("Starting…", phrase = true)
        // The host's steps are interface phrases ("Thinking…"); a title is the bot's own words.
        else -> if (activity.isEmpty()) TaskWhat(title, phrase = false) else TaskWhat(activity, phrase = true)
    }
}

/** A task's `<what>`: [text] is looked up in the app's language when it is a [phrase]. */
data class TaskWhat(val text: String, val phrase: Boolean)

@Serializable(with = TaskStatusSerializer::class)
enum class TaskStatus(val wire: String, val title: String) {
    Starting("starting", "Starting…"), Working("working", "Working"), NeedsInput("needsInput", "Needs you"),
    Done("done", "Done"), Failed("failed", "Failed"), Stopped("stopped", "Stopped");

    /** Still going: the ones a bot's `tasks` lists. */
    val live: Boolean get() = this == Starting || this == Working || this == NeedsInput
}

/** A status a newer host adds reads as working, rather than losing the bot. */
internal object TaskStatusSerializer : KSerializer<TaskStatus> {
    override val descriptor = PrimitiveSerialDescriptor("TaskStatus", PrimitiveKind.STRING)
    override fun serialize(encoder: Encoder, value: TaskStatus) = encoder.encodeString(value.wire)
    override fun deserialize(decoder: Decoder): TaskStatus {
        val wire = decoder.decodeString()
        return TaskStatus.entries.firstOrNull { it.wire == wire } ?: TaskStatus.Working
    }
}

/** A bot's `tasks`, read leniently: a list this app can't read is no tasks, a null field is a missing one, and a task it still
 *  can't read (no id) is left out, never a lost bot. */
internal object BotTasksSerializer : KSerializer<List<BotTask>> {
    private val list = ListSerializer(BotTask.serializer())
    override val descriptor = list.descriptor
    override fun serialize(encoder: Encoder, value: List<BotTask>) = list.serialize(encoder, value)
    override fun deserialize(decoder: Decoder): List<BotTask> {
        val json = decoder as? JsonDecoder ?: return list.deserialize(decoder)
        return (json.decodeJsonElement() as? JsonArray).orEmpty().mapNotNull { task ->
            val fields = (task as? JsonObject)?.filterValues { it !is JsonNull } ?: return@mapNotNull null
            runCatching { json.json.decodeFromJsonElement(BotTask.serializer(), JsonObject(fields)) }.getOrNull()?.takeIf { it.id.isNotEmpty() }
        }
    }
}

/** The capsule's line for a bot's [tasks] (not empty): what one is doing, or how many there are. The time is the oldest's. */
sealed interface TasksSummary {
    data class One(val what: TaskWhat) : TasksSummary
    data class Several(val count: Int, val needsInput: Boolean) : TasksSummary
}

fun tasksSummary(tasks: List<BotTask>): TasksSummary =
    tasks.singleOrNull()?.let { TasksSummary.One(it.what) }
        ?: TasksSummary.Several(tasks.size, tasks.any { it.status == TaskStatus.NeedsInput })

/**
 * The task transcripts being read, by task id, with [events]' entries laid over. Only a task whose details are open
 * has a key here: opening them fetches the transcript, so one nobody reads keeps nothing.
 */
fun Map<String, List<Entry>>.receivingTasks(events: List<MirrorEvent>): Map<String, List<Entry>> {
    val incoming = events.filterIsInstance<MirrorEvent.TaskEntryChanged>().filter { it.task in this }.groupBy({ it.task }, { it.entry })
    return if (incoming.isEmpty()) this else this + incoming.mapValues { (task, entries) -> mergeTaskEntries(getValue(task), entries) }
}

/** [held] with [incoming] laid over, oldest first: each entry once, the higher `rev` winning. */
fun mergeTaskEntries(held: List<Entry>, incoming: List<Entry>): List<Entry> =
    (held + incoming).groupBy { it.id }.map { (_, copies) -> copies.maxBy { it.rev } }.sortedWith(compareBy({ it.seq }, { it.id }))
