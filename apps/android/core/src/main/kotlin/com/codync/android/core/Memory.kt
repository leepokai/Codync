package com.codync.android.core

import kotlinx.serialization.Serializable

// A bot's memory as its screen shows it, read-only (iOS `Memory.swift`, `MemoryFiles.swift`): the user changes it by telling the bot.

/** One line of a layer's `MEMORY.md`. */
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

/** One dreaming run: what it added and archived (`undone` when an older app undid it). */
@Serializable data class DreamRun(val id: Long, val at: Long, val undone: Boolean = false, val added: List<Change> = emptyList(),
    val archived: List<Change> = emptyList(), val files: List<FileNote>? = null) {
    @Serializable data class Change(val id: Long, val text: String)
    /** A long-form note the run added a line to. */
    @Serializable data class FileNote(val name: String, val shared: Boolean = false, val version: Long = 0)
}

/** A long-form memory note: a Markdown file on one topic, as the host last recorded it. */
@Serializable data class MemoryFile(val bot: String? = null, val name: String, val description: String? = null, val tokens: Int = 0,
    val deleted: Boolean = false, val updatedAt: Long = 0)

/** One layer's folder of notes, and where it is on the computer. */
@Serializable data class MemoryFolder(val files: List<MemoryFile> = emptyList(), val folder: String = "")

/** Both layers' notes, with the caps the host marks: tokens per note, notes per folder. */
@Serializable data class MemoryFiles(val yours: MemoryFolder = MemoryFolder(), val bot: MemoryFolder = MemoryFolder(), val cap: Int = 0, val folderCap: Int = 0)

/** A note as it is on disk now; [content] null when it's gone. */
@Serializable data class MemoryFileDetail(val file: String, val shared: Boolean = false, val content: String? = null, val path: String = "")

/** A bot's memory (`memory`): what all bots share about the user, what this one learned, its dreams and its harness's own notes. */
@Serializable data class BotMemory(
    /** Whether this bot uses what all bots share. */
    val shared: Boolean,
    val yours: MemoryLayer,
    val bot: MemoryLayer,
    val dreams: List<DreamRun>,
    /** Claude Code's own notes for the bot's project folder, read-only. */
    val projectMemory: Project? = null,
    val files: MemoryFiles? = null,
) {
    @Serializable data class Project(val path: String, val text: String = "")
}
