package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** One memory entry (CodyncKit `MemoryEntry`). */
@Serializable data class MemoryEntry(
    val id: Long,
    /** The bot whose own layer holds it; null for what all bots share. */
    val bot: String? = null,
    /** `active` or `archived`. */
    val state: String = "active",
    val text: String,
    /** `user`, `bot`, `dream` or `imported`. */
    val author: String = "user",
    val authorBot: String? = null,
    val pinned: Boolean = false,
    val updatedAt: Long = 0,
) {
    val archived: Boolean get() = state == "archived"
}

@Serializable data class MemoryLayer(val entries: List<MemoryEntry> = emptyList(), val used: Int = 0, val cap: Int = 0)

/** One dreaming run: what it added and archived. It can be undone. */
@Serializable data class DreamRun(
    val id: Long,
    val at: Long = 0,
    val undone: Boolean = false,
    val added: List<Change> = emptyList(),
    val archived: List<Change> = emptyList(),
    val files: List<FileNote>? = null,
) {
    @Serializable data class Change(val id: Long, val text: String)
    /** A long-form file the run added a line to. */
    @Serializable data class FileNote(val name: String, val shared: Boolean = false, val version: Long = 0)
}

/** A long-form memory file: a Markdown note on one topic, as the host last recorded it. */
@Serializable data class MemoryFile(
    val bot: String? = null,
    val name: String,
    /** SHA-256 of what it holds; a save is made over it. */
    val hash: String = "",
    val description: String? = null,
    val tokens: Int = 0,
    val deleted: Boolean = false,
    val updatedAt: Long = 0,
)

@Serializable data class MemoryFolder(val files: List<MemoryFile> = emptyList(), val folder: String = "")

/** Both layers' files, with the caps the host marks. */
@Serializable data class MemoryFiles(val yours: MemoryFolder, val bot: MemoryFolder, val cap: Int = 0, val folderCap: Int = 0)

/** One version of a note: who made it, why, and what it held then. */
@Serializable data class MemoryFileVersion(
    val id: Long,
    /** Null: the file was deleted. */
    val content: String? = null,
    /** `user`, `bot`, `dream` or `computer`. */
    val author: String = "computer",
    val authorBot: String? = null,
    /** `edit`, `delete`, `restore`, `undo` or `dream`. */
    val cause: String = "edit",
    val at: Long = 0,
)

/** A note as it is on disk now, with its recent versions. */
@Serializable data class MemoryFileDetail(
    val file: String,
    val shared: Boolean = false,
    val content: String? = null,
    /** The hash to save over; null when the file is gone. */
    val hash: String? = null,
    val path: String = "",
    val versions: List<MemoryFileVersion> = emptyList(),
)

/** A bot's memory as its screen shows it (CodyncKit `MemoryView`); an older host's reply has no layers and fails to decode. */
@Serializable data class BotMemory(
    /** Whether this bot uses what all bots share. */
    val shared: Boolean = false,
    val yours: MemoryLayer,
    val bot: MemoryLayer,
    val dreams: List<DreamRun> = emptyList(),
    /** Claude Code's own notes for the bot's project folder, read-only. */
    val projectMemory: Project? = null,
    val files: MemoryFiles? = null,
) {
    @Serializable data class Project(val path: String, val text: String)
}

/** What the memory editor opens on: an entry, or (`entry` null) a new one in a layer. */
data class MemoryEditTarget(val entry: Long?, val shared: Boolean, val text: String)

/** What a note sheet opens: a file, or (`name` null) a new one. */
data class MemoryFileTarget(val name: String?, val shared: Boolean)

/** The request bodies of the memory methods (`saveMemory`, `pinMemory`, `archiveMemory`, `forgetMemory`, `clearMemory`, `undoDream`). */
object MemoryCalls {
    fun read(botId: String): JsonObject = buildJsonObject { put("botId", botId) }

    fun mutation(botId: String, id: Long? = null, pinned: Boolean? = null, archived: Boolean? = null, shared: Boolean? = null,
        text: String? = null): JsonObject = buildJsonObject {
        put("botId", botId)
        id?.let { put("id", it) }
        pinned?.let { put("pinned", it) }
        archived?.let { put("archived", it) }
        shared?.let { put("shared", it) }
        text?.let { put("text", it) }
    }

    /** Saves (`content`), deletes (`content` and `version` null) or brings back a version of a note, over the content whose hash is `base`. */
    fun fileMethod(content: String?, version: Long?): String =
        if (version != null) "restoreMemoryFile" else if (content != null) "saveMemoryFile" else "deleteMemoryFile"

    fun fileChange(botId: String, file: String, shared: Boolean, base: String?, content: String?, version: Long?): JsonObject = buildJsonObject {
        put("botId", botId); put("file", file); put("shared", shared)
        base?.let { put("base", it) }
        content?.let { put("content", it) }
        version?.let { put("version", it) }
    }

    fun fileRead(botId: String, file: String, shared: Boolean): JsonObject = buildJsonObject {
        put("botId", botId); put("file", file); put("shared", shared)
    }
}
