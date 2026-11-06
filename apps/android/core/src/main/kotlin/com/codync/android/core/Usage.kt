package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.decodeFromJsonElement
import java.time.Instant
import java.time.MonthDay
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

@Serializable data class UsageReport(val providers: List<UsageProvider> = emptyList()) {
    companion object { fun decode(value: JsonElement): UsageReport = WireJson.decodeFromJsonElement(value) }
}

@Serializable data class UsageProvider(val id: String, val name: String, val windows: List<UsageWindow> = emptyList(),
    val source: String = "", val updatedAt: Long? = null) {
    val limits: List<UsageWindow> get() = windows.filter { it.text.isNullOrBlank() }
    val tightest: UsageWindow? get() = limits.filter { it.percent?.isFinite() == true }.maxByOrNull { requireNotNull(it.percent) }
    val sourceLabel: String get() = when (source) {
        "claude" -> "Claude Code"; "statusline" -> "Claude Code status line"
        "agent" -> "A running bot"; "sessions" -> "Codex sessions"; "" -> "Unknown source"; else -> source
    }
}

@Serializable data class UsageWindow(val id: String, val label: String, val percent: Double? = null,
    val resetsAt: Long? = null, val resetsText: String? = null, val text: String? = null) {
    val reportedPercent: Double? get() = percent?.takeIf { it.isFinite() }
    val barFraction: Float? get() = reportedPercent?.coerceIn(0.0, 100.0)?.div(100)?.toFloat()

    /** When the window resets: `resetsAt`, or Claude Code's text read as a date. */
    fun resetDate(now: Long = System.currentTimeMillis()): Long? = resetsAt?.takeIf { it > 0 } ?: resetsText?.let { parseReset(it, now) }

    companion object {
        private val claudeReset = Regex("""^(?:(\w{3} \d{1,2}) at )?(\d{1,2})(?::(\d{2}))?(am|pm)(?: \(([^)]+)\))?$""")

        /**
         * Claude Code's text: "Sep 26 at 12pm (Asia/Taipei)", "Sep 25 at 7:30pm (Asia/Taipei)", or just
         * "7:30pm (…)". The year isn't given: the next such date from `now` (CodyncKit `parseReset`).
         */
        fun parseReset(text: String, now: Long = System.currentTimeMillis()): Long? {
            val match = claudeReset.matchEntire(text) ?: return null
            val (day, hour12, minute, half, zoneId) = match.destructured
            val zone = zoneId.takeIf(String::isNotEmpty)?.let { runCatching { ZoneId.of(it) }.getOrNull() } ?: ZoneId.systemDefault()
            val today = Instant.ofEpochMilli(now).atZone(zone).toLocalDate()
            val date = if (day.isEmpty()) today else runCatching {
                MonthDay.parse(day, DateTimeFormatter.ofPattern("MMM d", Locale.US)).atYear(today.year)
            }.getOrNull() ?: return null
            val hour = (hour12.toIntOrNull() ?: return null) % 12 + if (half == "pm") 12 else 0
            var at = date.atTime(hour, minute.toIntOrNull() ?: 0).atZone(zone)
            // Past by more than a day: it means next year (or tomorrow, for a bare time).
            val millis = at.toInstant().toEpochMilli()
            if (millis < now - 86_400_000 || (day.isEmpty() && millis < now)) at = if (day.isEmpty()) at.plusDays(1) else at.plusYears(1)
            return at.toInstant().toEpochMilli()
        }
    }
}
