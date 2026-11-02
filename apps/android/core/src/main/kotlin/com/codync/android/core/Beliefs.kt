package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.jsonObject

@Serializable data class BeliefVersion(val id: Long, val content: String? = null, val cause: String = "edit", val at: Long = 0)

/** One note of a bot's memory as a belief, as `beliefs` lists it. */
@Serializable data class Belief(
    val file: String,
    val shared: Boolean = false,
    val description: String? = null,
    /** `user`, `feedback`, `project` or `reference`. */
    val kind: String = "user",
    /** `high`, `medium` or `low`. */
    val confidence: String = "medium",
    /** `YYYY-MM-DD`, or null when nobody confirmed it. */
    val confirmed: String? = null,
    val versions: List<BeliefVersion> = emptyList(),
) {
    /** A version that can be brought back: not the newest one, and not a forgetting. */
    fun historyRows(): List<HistoryRow> = versions.mapIndexed { i, v ->
        HistoryRow(v.id, CAUSES[v.cause] ?: "Changed", v.at, v.content != null && i > 0)
    }

    data class HistoryRow(val id: Long, val label: String, val at: Long, val restorable: Boolean)

    companion object {
        val KINDS = listOf("user" to "About you", "feedback" to "How to work", "project" to "Projects", "reference" to "References")
        val CONFIDENCES = listOf("high" to "Sure", "medium" to "Probably", "low" to "Unsure")
        private val CAUSES = mapOf("edit" to "Changed", "delete" to "Forgotten", "restore" to "Brought back", "undo" to "Undone", "dream" to "Tidied while dreaming")

        fun decodeAll(response: JsonElement): List<Belief> =
            WireJson.decodeFromJsonElement(kotlinx.serialization.builtins.ListSerializer(serializer()), response.jsonObject.getValue("beliefs"))

        fun kindLabel(kind: String) = KINDS.firstOrNull { it.first == kind }?.second ?: "About you"
        fun confidenceLabel(confidence: String) = CONFIDENCES.firstOrNull { it.first == confidence }?.second ?: "Probably"
        fun confirmedText(confirmed: String?) = if (confirmed == null) "Never confirmed" else "Confirmed $confirmed"

        /** `null` for a filter means all. */
        fun filter(beliefs: List<Belief>, kind: String?, confidence: String?): List<Belief> =
            beliefs.filter { (kind == null || it.kind == kind) && (confidence == null || it.confidence == confidence) }

        /** The unsure first, then the longest unconfirmed. */
        fun byAttention(beliefs: List<Belief>): List<Belief> {
            val rank = mapOf("low" to 0, "medium" to 1, "high" to 2)
            return beliefs.sortedWith(compareBy<Belief>({ rank[it.confidence] ?: 1 }, { it.confirmed ?: "" }, { it.file }))
        }
    }
}
