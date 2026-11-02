package com.codync.android

import android.util.AtomicFile
import com.codync.android.core.Bot
import com.codync.android.core.WireJson
import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.encodeToString

@Serializable
data class TaskWatch(val taskId: String = java.util.UUID.randomUUID().toString(), val botId: String, val name: String, val status: String = "working", val finished: Boolean = false,
    val seenRunning: Boolean = false, val timestamp: Long = 0, val lastRev: Long = 0, val deadline: Long = System.currentTimeMillis() + 900_000,
    val startedAt: Long = System.currentTimeMillis(), val avatarShape: String = "blob", val avatarColor: String = "blue", val activity: String = "")

/** One computer's bounded delegation markers; inactive contexts never render or accept these updates. */
class TaskFeed(directory: File, computerId: String) {
    private val file = AtomicFile(File(directory, "tasks-$computerId.json"))
    fun all(): List<TaskWatch> = synchronized(lock) {
        if (!file.baseFile.exists()) emptyList() else WireJson.decodeFromString<List<TaskWatch>>(file.readFully().toString(Charsets.UTF_8))
    }
    fun clear() = synchronized(lock) { file.delete() }
    fun start(bot: Bot): TaskWatch = synchronized(lock) {
        val running = bot.status in setOf("working", "needsInput")
        val watch = TaskWatch(botId = bot.id, name = bot.name, lastRev = bot.rev, seenRunning = running,
            status = if (running) bot.status else "working", avatarShape = bot.avatarShape, avatarColor = bot.avatarColor)
        save((all().filter { it.botId != bot.id } + watch).takeLast(16))
        watch
    }
    fun local(bot: Bot): TaskWatch? = synchronized(lock) {
        val watch = all().firstOrNull { it.botId == bot.id && !it.finished } ?: return@synchronized null
        if (bot.rev <= watch.lastRev) return@synchronized null
        if (bot.status !in setOf("working", "needsInput", "idle", "error") || bot.status == "idle" && !watch.seenRunning) return@synchronized null
        val updated = watch.copy(name = bot.name, status = bot.status, finished = bot.status in setOf("idle", "error"), activity = bot.activity,
            seenRunning = true, lastRev = bot.rev, deadline = System.currentTimeMillis() + 900_000)
        save(all().map { if (it.botId == bot.id) updated else it })
        updated
    }
    fun remote(data: Map<String, String>, botId: String): TaskWatch? = synchronized(lock) {
        val watch = all().firstOrNull { it.botId == botId && it.taskId == data["taskId"] && !it.finished } ?: return@synchronized null
        val status = data["status"]?.takeIf { it in setOf("working", "needsInput", "idle", "error") } ?: return@synchronized null
        val timestamp = data["timestamp"]?.toLongOrNull() ?: return@synchronized null
        val stale = data["staleDate"]?.toLongOrNull() ?: return@synchronized null
        val now = System.currentTimeMillis() / 1000
        if (timestamp < 0 || timestamp > now + 60 || stale < timestamp || stale > timestamp + 3600 || stale <= now ||
            timestamp < watch.timestamp || watch.finished && timestamp <= watch.timestamp) return@synchronized null
        val event = data["event"] ?: return@synchronized null
        if (event !in setOf("update", "end") || (event == "end") != (status in setOf("idle", "error"))) return@synchronized null
        val updated = watch.copy(status = status, finished = event == "end", seenRunning = true, timestamp = timestamp, deadline = stale * 1000)
        save(all().map { if (it.botId == botId) updated else it })
        updated
    }
    private fun save(watches: List<TaskWatch>) {
        val output = file.startWrite()
        try { output.write(WireJson.encodeToString(watches).toByteArray()); file.finishWrite(output) }
        catch (error: Throwable) { file.failWrite(output); throw error }
    }
    companion object { private val lock = Any() }
}
