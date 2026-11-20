package com.codync.android.core

import kotlinx.serialization.json.*

/** The last part of a folder path, both separators (a Windows host's folders). */
fun folderName(path: String): String = path.split('/', '\\').lastOrNull { it.isNotEmpty() } ?: path

/** A group uses the existing bot endpoints; it never owns an agent. It may carry a shared workspace folder. */
data class GroupDraft(
    val id: String? = null,
    val name: String = "",
    val description: String = "",
    val members: List<String> = emptyList(),
    val pinned: Boolean = false,
    /** The team's shared workspace folder; sent only when set (null alone keeps the saved one). */
    val workspace: String? = null,
    /** Clears the team's shared folder, sent as explicit null (only with [workspace] null). */
    val clearWorkspace: Boolean = false,
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
            if (workspace != null) put("workspace", workspace)
            else if (clearWorkspace) put("workspace", JsonNull)
        }
    }

    companion object {
        fun from(bot: Bot): GroupDraft {
            require(bot.kind == "group")
            return GroupDraft(bot.id, bot.name, bot.description, bot.members, bot.pinned, bot.workspace)
        }
    }
}

