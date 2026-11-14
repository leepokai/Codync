package com.codync.android.core

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull

/**
 * One thing in a chat's Library (`host/src/chat/library.rs`), `at` when it was last published or shared: an artifact, a
 * file the bot shared (read with `readFile`) or a file the user attached (read with `readUpload`).
 */
sealed interface LibraryItem {
    val at: Long

    data class ArtifactItem(override val at: Long, val artifact: Artifact) : LibraryItem
    data class FileItem(override val at: Long, val entryId: String, val file: SharedFile) : LibraryItem
    data class AttachmentItem(override val at: Long, val entryId: String, val file: Attachment) : LibraryItem

    companion object {
        /** An item, or null for a kind this app doesn't know (or can't read): the rest of the Library still shows. */
        fun decode(value: JsonObject): LibraryItem? = runCatching {
            val at = requireNotNull(value["at"]?.jsonPrimitive?.longOrNull)
            val file = value["file"]
            when (value.text("kind")) {
                "artifact" -> ArtifactItem(at, Artifact.decode(value.getValue("artifact")))
                "file" -> FileItem(at, requireNotNull(value.text("entryId")), WireJson.decodeFromJsonElement(requireNotNull(file)))
                "attachment" -> AttachmentItem(at, requireNotNull(value.text("entryId")), WireJson.decodeFromJsonElement(requireNotNull(file)))
                else -> null
            }
        }.getOrNull()
    }
}

/** Where an item falls in the Library, by the day it was published or shared. */
enum class LibraryPeriod(val title: String) {
    Today("Today"), Yesterday("Yesterday"), ThisWeek("This week"), Earlier("Earlier");

    companion object {
        fun of(at: Long, today: LocalDate, zone: ZoneId): LibraryPeriod {
            val day = Instant.ofEpochMilli(at).atZone(zone).toLocalDate()
            return when {
                !day.isBefore(today) -> Today
                day == today.minusDays(1) -> Yesterday
                !day.isBefore(today.minusDays(6)) -> ThisWeek
                else -> Earlier
            }
        }

        /** Items already newest first, in their periods, in that order; empty periods left out. */
        fun sections(items: List<LibraryItem>, today: LocalDate = LocalDate.now(), zone: ZoneId = ZoneId.systemDefault()): List<Pair<LibraryPeriod, List<LibraryItem>>> =
            items.groupBy { of(it.at, today, zone) }.toList().sortedBy { it.first.ordinal }
    }
}
