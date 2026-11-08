package com.codync.android

import android.content.Context
import java.io.File
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

/** What a support log line is about; the level says how loud it is. */
internal enum class SupportLevel { Info, Warn, Error }
internal enum class SupportSource { App, Network, Connection }

internal data class SupportEvent(val id: Long, val at: Long, val level: SupportLevel, val source: SupportSource, val message: String)

private val supportClock = DateTimeFormatter.ofPattern("HH:mm:ss")

/**
 * Recent app events for the Support page: connection steps, each request with its outcome and
 * errors, redacted and capped at 5,000 lines. It stays on the phone (app cache, never backed up)
 * until the person saves it from Settings → Support or clears it.
 */
internal object SupportLog {
    private const val MAX_EVENTS = 5_000
    private const val MAX_MESSAGE = 600
    private const val SAVE_EVERY_MS = 5_000L

    private val _events = MutableStateFlow<List<SupportEvent>>(emptyList())
    /** Oldest first. */
    val events = _events.asStateFlow()
    private var nextId = 0L
    private var file: File? = null
    @Volatile private var dirty = false
    private val writer = Executors.newSingleThreadScheduledExecutor { runnable ->
        Thread(runnable, "support-log").apply { isDaemon = true }
    }

    fun start(context: Context) {
        if (file != null) return
        val target = File(context.cacheDir, "support-log.txt")
        file = target
        val restored = runCatching {
            target.readLines().takeLast(MAX_EVENTS).mapNotNull { line ->
                val at = line.substringBefore(' ').let { runCatching { Instant.parse(it).toEpochMilli() }.getOrNull() } ?: return@mapNotNull null
                val level = SupportLevel.entries.firstOrNull { line.contains(" ${it.name.uppercase()} ") } ?: SupportLevel.Info
                val source = SupportSource.entries.firstOrNull { line.contains(" ${it.name.uppercase()} ") } ?: SupportSource.App
                SupportEvent(nextId++, at, level, source, line.substringAfter(" ${source.name.uppercase()} ", ""))
            }
        }.getOrDefault(emptyList())
        if (restored.isNotEmpty()) _events.value = restored
        writer.scheduleWithFixedDelay(::save, SAVE_EVERY_MS, SAVE_EVERY_MS, TimeUnit.MILLISECONDS)
    }

    fun add(level: SupportLevel, source: SupportSource, message: String) {
        val text = redact(message)
        val event = SupportEvent(nextId++, System.currentTimeMillis(), level, source,
            if (text.length > MAX_MESSAGE) text.take(MAX_MESSAGE) + "…" else text)
        _events.value = (_events.value + event).takeLast(MAX_EVENTS)
        dirty = true
    }

    fun info(source: SupportSource, message: String) = add(SupportLevel.Info, source, message)
    fun warn(source: SupportSource, message: String) = add(SupportLevel.Warn, source, message)
    fun error(source: SupportSource, message: String) = add(SupportLevel.Error, source, message)

    fun clear() {
        _events.value = emptyList()
        // The next save writes the empty file.
        dirty = true
    }

    /** The saved file's text, newest last, each line stamped and redacted. */
    fun fileText(): String = _events.value.joinToString("\n") {
        "${Instant.ofEpochMilli(it.at)} ${it.level.name.uppercase()} ${it.source.name.uppercase()} ${redact(it.message)}"
    }

    private fun save() {
        if (!dirty) return
        dirty = false
        val target = file ?: return
        runCatching { target.writeText(fileText()) }
    }

    private val email = Regex("[\\w.+-]+@[\\w-]+(\\.[\\w-]+)+")
    private val bearer = Regex("(?i)(bearer\\s+)\\S+")
    private val secret = Regex("\\b(sk|pk|ghp|gho|ghs|xoxb|xoxp|shpat|glpat)_[A-Za-z0-9_]{6,}")
    private val longToken = Regex("(?<![\\w/~-])[A-Za-z0-9_~+/=-]{28,}(?![\\w/~-])")

    /** Masks tokens, keys and addresses as the desktop logs do, and keeps each message on one line. */
    internal fun redact(text: String): String = text
        .replace(bearer) { "${it.groupValues[1]}•••" }
        .replace(secret) { "${it.groupValues[1]}_${it.value.take(4)}…" }
        .replace(longToken) { it.value.take(4) + "…" }
        .replace(email) { "${it.value.substringBefore('@').take(2)}…@${it.value.substringAfter('@')}" }
        .replace(Regex("[\\r\\n]+\\s*"), " | ")
        .replace(Regex("\\p{Cc}"), " ")
}

/** A stamp for a support log line in the phone's zone, e.g. `14:32:07`. */
internal fun SupportEvent.clock(): String = Instant.ofEpochMilli(at).atZone(ZoneId.systemDefault()).format(supportClock)
