package com.codync.android.core

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.*

/** A group uses the existing bot endpoints; it never owns an agent or workspace. */
data class GroupDraft(
    val id: String? = null,
    val name: String = "",
    val description: String = "",
    val members: List<String> = emptyList(),
    val pinned: Boolean = false,
    val chiefBotId: String? = null,
) {
    fun body(roster: List<Bot>): JsonObject {
        val picked = members.distinct().map { id ->
            requireNotNull(roster.firstOrNull { it.id == id && !it.deleted && it.kind != "group" }) {
                "A member is no longer available. Remove it before saving."
            }
        }
        require(picked.isNotEmpty()) { "Choose at least one bot." }
        return buildJsonObject {
            id?.let { put("id", it) }
            put("kind", "group")
            put("name", name.trim().ifEmpty { picked.joinToString(", ") { it.name } })
            put("description", description.trim())
            put("members", JsonArray(picked.map { JsonPrimitive(it.id) }))
            put("pinned", pinned)
            put("chiefBotId", chiefBotId?.let { JsonPrimitive(it) } ?: JsonNull)
        }
    }

    companion object {
        fun from(bot: Bot): GroupDraft {
            require(bot.kind == "group")
            return GroupDraft(bot.id, bot.name, bot.description, bot.members, bot.pinned, bot.chiefBotId)
        }
    }
}
