package com.codync.android.core

import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

/**
 * A workflow on the connected computer. Field names match the host JSON.
 * An unrecognized step kind decodes as [NodeKind.Unknown] so one detail can stay read-only
 * without failing the rest of the sync.
 */
@Serializable
enum class NodeKind {
    Trigger, Prompt, Agent, Condition, Approval, Output, Text, Note, Shape, Unknown;

    val wire: String
        get() = when (this) {
            Trigger -> "trigger"
            Prompt -> "prompt"
            Agent -> "agent"
            Condition -> "condition"
            Approval -> "approval"
            Output -> "output"
            Text -> "text"
            Note -> "note"
            Shape -> "shape"
            Unknown -> "unknown"
        }

    val isAnnotation: Boolean get() = this == Text || this == Note || this == Shape
    val isStep: Boolean get() = !isAnnotation && this != Unknown

    companion object {
        fun fromWire(raw: String): NodeKind = entries.firstOrNull { it.wire == raw } ?: Unknown
    }
}

object NodeKindSerializer : KSerializer<NodeKind> {
    override val descriptor = PrimitiveSerialDescriptor("NodeKind", PrimitiveKind.STRING)
    override fun deserialize(decoder: Decoder): NodeKind = NodeKind.fromWire(decoder.decodeString())
    override fun serialize(encoder: Encoder, value: NodeKind) { encoder.encodeString(value.wire) }
}

/** How the phone may change a graph. Comes from the host detail; an unknown kind forces [ReadOnly]. */
enum class EditMode { Outline, Fields, Script, ReadOnly }

/** What a node accepts and emits. Annotations have neither. The only port is `text`. */
fun ports(kind: NodeKind): Pair<List<String>, List<String>> = when (kind) {
    NodeKind.Trigger, NodeKind.Prompt -> emptyList<String>() to listOf("text")
    NodeKind.Agent, NodeKind.Condition, NodeKind.Approval -> listOf("text") to listOf("text")
    NodeKind.Output -> listOf("text") to emptyList()
    NodeKind.Text, NodeKind.Note, NodeKind.Shape, NodeKind.Unknown -> emptyList<String>() to emptyList()
}

@Serializable data class Point(val x: Double = 0.0, val y: Double = 0.0)
@Serializable data class Size(val width: Double = 0.0, val height: Double = 0.0)

@Serializable
data class WorkflowNode(
    val id: String,
    @Serializable(with = NodeKindSerializer::class) val kind: NodeKind,
    val at: Point = Point(),
    val size: Size? = null,
    val locked: Boolean = false,
    val fields: JsonObject = JsonObject(emptyMap()),
    val schedule: JsonObject? = null,
)

@Serializable
data class WorkflowEdge(
    val id: String,
    val from: String,
    val to: String,
    @kotlinx.serialization.SerialName("type") val type: String = "text",
    val branch: String? = null,
)

@Serializable
data class WorkflowStroke(
    val id: String,
    val brush: String = "pen",
    val color: String = "",
    val width: Double = 0.0,
    val points: List<Point> = emptyList(),
)

@Serializable
data class Graph(
    val nodes: List<WorkflowNode> = emptyList(),
    val edges: List<WorkflowEdge> = emptyList(),
    val strokes: List<WorkflowStroke> = emptyList(),
)

@Serializable
data class WorkflowLastRun(val number: Long = 0, val status: String = "", val endedAt: Long? = null)

@Serializable
data class WorkflowSummary(
    val id: String,
    val title: String = "",
    val revision: Long = 0,
    val enabled: Boolean = true,
    val updatedAt: Long = 0,
    val createdAt: Long = 0,
    val running: String? = null,
    val lastRun: WorkflowLastRun? = null,
)

@Serializable
data class WorkflowWebhook(val nodeId: String = "", val url: String = "", val secret: String? = null)

@Serializable
data class WorkflowDetail(
    val id: String,
    val title: String = "",
    val revision: Long = 0,
    val enabled: Boolean = true,
    val updatedAt: Long = 0,
    val createdAt: Long = 0,
    val running: String? = null,
    val lastRun: WorkflowLastRun? = null,
    val graph: Graph = Graph(),
    val editMode: String = "fields",
    val webhooks: List<WorkflowWebhook> = emptyList(),
) {
    fun summary(): WorkflowSummary = WorkflowSummary(id, title, revision, enabled, updatedAt, createdAt, running, lastRun)
}

@Serializable data class WorkflowList(val workflows: List<WorkflowSummary> = emptyList())

@Serializable
data class WorkflowVersion(val id: String, val revision: Long = 0, val kind: String = "auto", val createdAt: Long = 0)

@Serializable data class WorkflowVersionList(val versions: List<WorkflowVersion> = emptyList())

@Serializable
data class WorkflowSaveResult(
    val ok: Boolean = false,
    val conflict: Boolean = false,
    val revision: Long = 0,
    val workflow: WorkflowDetail? = null,
)

@Serializable
data class WorkflowRunSummary(
    val id: String,
    val number: Long = 0,
    val source: String = "",
    val status: String = "",
    val startedAt: Long = 0,
    val endedAt: Long? = null,
    val error: String? = null,
)

@Serializable data class WorkflowRunPage(val runs: List<WorkflowRunSummary> = emptyList(), val hasMore: Boolean = false)

@Serializable
data class WorkflowStep(
    val id: String,
    val nodeId: String = "",
    val attempt: Int = 1,
    val requestId: String = "",
    val kind: String = "",
    val botId: String? = null,
    val status: String = "",
    val input: String = "",
    val output: String? = null,
    val branch: String? = null,
    val passes: Int = 1,
    val truncated: Boolean = false,
    val error: String? = null,
    val startedAt: Long? = null,
    val endedAt: Long? = null,
    val title: String = "",
)

@Serializable
data class WorkflowRunDetail(
    val id: String,
    val number: Long = 0,
    val source: String = "",
    val status: String = "",
    val startedAt: Long = 0,
    val endedAt: Long? = null,
    val error: String? = null,
    val graph: Graph = Graph(),
    val steps: List<WorkflowStep> = emptyList(),
)

/** A live `workflow` event. It carries no `rev`; a body that does not parse is ignored. */
data class WorkflowChanged(
    val change: String,
    val workflowId: String,
    val runId: String? = null,
    val stepId: String? = null,
    val enabled: Boolean? = null,
)

private val OPEN_RUN = setOf("running", "approval", "stopping")

/** In-progress workflows first, then most recently updated. */
fun List<WorkflowSummary>.listed(): List<WorkflowSummary> =
    sortedWith(compareByDescending<WorkflowSummary> { it.running in OPEN_RUN }.thenByDescending { it.updatedAt }.thenBy { it.id })

/** List and pinned-row subtitle: Running, Needs you, Done, Failed, Stopped, or none. */
fun WorkflowSummary.statusLabel(): String? {
    fun label(status: String?) = when (status) {
        "running", "stopping" -> "Running"
        "approval" -> "Needs you"
        "done" -> "Done"
        "failed" -> "Failed"
        "stopped" -> "Stopped"
        else -> null
    }
    return label(running) ?: label(lastRun?.status)
}

fun stepStatusLabel(status: String): String = when (status) {
    "running" -> "Running"
    "approval" -> "Needs you"
    "done" -> "Done"
    "failed" -> "Failed"
    "stopped" -> "Stopped"
    "skipped" -> "Skipped"
    else -> status.replaceFirstChar { it.uppercase() }
}

fun runStatusLabel(status: String): String = when (status) {
    "running", "stopping" -> "Running"
    "approval" -> "Needs you"
    else -> stepStatusLabel(status)
}

/**
 * The host's edit mode, except a kind or port this app cannot round-trip: that detail is read-only
 * so a save cannot drop it.
 */
fun WorkflowDetail.resolvedMode(): EditMode {
    val readable = graph.nodes.none { it.kind == NodeKind.Unknown } &&
        graph.edges.all { it.type == "text" && (it.branch == null || it.branch == "yes" || it.branch == "no") }
    if (!readable) return EditMode.ReadOnly
    return when (editMode) {
        "outline" -> EditMode.Outline
        "script" -> EditMode.Script
        "readOnly" -> EditMode.ReadOnly
        else -> EditMode.Fields
    }
}

fun JsonObject.field(key: String): String? = (this[key] as? JsonPrimitive)?.contentOrNull

fun JsonObject.withField(key: String, value: String?): JsonObject {
    val next = toMutableMap()
    if (value == null) next.remove(key) else next[key] = JsonPrimitive(value)
    return JsonObject(next)
}

private fun String?.firstLine(): String? = this?.trim()?.lineSequence()?.firstOrNull { it.isNotBlank() }

/** Step title on the phone: a bot's name, a trigger's kind, or the first line of its text. */
fun WorkflowNode.stepTitle(botName: (String) -> String? = { null }): String = when (kind) {
    NodeKind.Trigger -> when (fields.field("trigger")) {
        "schedule" -> "Schedule"
        "webhook" -> "Webhook"
        else -> "Manual"
    }
    NodeKind.Prompt -> fields.field("text").firstLine() ?: "Prompt"
    NodeKind.Agent -> fields.field("botId")?.let(botName) ?: "Assistant"
    NodeKind.Condition -> "Condition"
    NodeKind.Approval -> fields.field("question").firstLine() ?: "Approval"
    NodeKind.Output -> if (fields.field("target") == "chat") fields.field("botId")?.let(botName) ?: "Choose a bot" else "Notification"
    NodeKind.Text, NodeKind.Note, NodeKind.Shape, NodeKind.Unknown -> kind.wire.replaceFirstChar { it.uppercase() }
}

/** One line under the title: the bot, Manual, the next run, or Notification. */
fun WorkflowNode.stepSummary(botName: (String) -> String? = { null }, schedule: String? = null): String = when (kind) {
    NodeKind.Trigger -> when (fields.field("trigger")) {
        "schedule" -> schedule ?: "Schedule"
        "webhook" -> "Webhook"
        else -> "Manual"
    }
    NodeKind.Agent -> fields.field("botId")?.let(botName) ?: "Choose a bot"
    NodeKind.Output -> if (fields.field("target") == "chat") fields.field("botId")?.let(botName) ?: "Choose a bot" else "Notification"
    NodeKind.Prompt -> fields.field("text").firstLine().orEmpty()
    NodeKind.Condition -> (fields.field("op") ?: "contains").replaceFirstChar { it.uppercase() }
    NodeKind.Approval -> fields.field("question").firstLine() ?: "Approve this step?"
    else -> ""
}

fun stepSymbol(kind: NodeKind): String = when (kind) {
    NodeKind.Trigger -> "bolt.fill"
    NodeKind.Prompt -> "text.alignleft"
    NodeKind.Agent -> "person.fill"
    NodeKind.Condition -> "arrow.triangle.branch"
    NodeKind.Approval -> "checkmark.seal"
    NodeKind.Output -> "bell.fill"
    NodeKind.Text -> "textformat"
    NodeKind.Note -> "note.text"
    NodeKind.Shape -> "square.on.circle"
    NodeKind.Unknown -> "questionmark"
}
