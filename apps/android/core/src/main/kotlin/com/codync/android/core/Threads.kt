package com.codync.android.core

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject

/**
 * A chat's thread as the list shows it (host `threads`): never its lines, only where it ends and
 * what it was branched from. A thread is a side conversation of the chat; its replies stay out of it.
 */
@Serializable
data class ThreadInfo(
    val id: String,
    val title: String = "",
    /** The message the thread was branched from, for "Branched from …". */
    @SerialName("fromEntry") val fromEntry: String? = null,
    /** That message's text, as the computer quoted it. */
    @SerialName("branchText") val branchText: String? = null,
    val archived: Boolean = false,
    @SerialName("createdAt") val createdAt: Long = 0,
    @SerialName("updatedAt") val updatedAt: Long = 0,
    val unread: Int = 0,
    @SerialName("lastText") val lastText: String? = null,
    @SerialName("lastAuthor") val lastAuthor: String? = null,
) {
    /** What the list calls it: its title, or the line it was branched from, or "Thread". */
    val name: String get() = title.ifBlank { branchText?.take(60)?.replace('\n', ' ') ?: "Thread" }

    companion object {
        fun decode(value: JsonObject): ThreadInfo = WireJson.decodeFromJsonElement(value)
        fun decodeList(response: JsonElement): List<ThreadInfo> =
            response.jsonObject["threads"]?.jsonArray?.map { decode(it.jsonObject) }.orEmpty()
    }
}
