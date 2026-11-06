package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.*
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/** Schedules execute on the host. Raw triggers retain fields the mobile editor does not edit. */
@Serializable data class Routine(
    val id: String, val botId: String, val name: String, val instruction: String,
    val triggers: List<JsonObject>, val enabled: Boolean,
    val createdAt: Long = 0, val updatedAt: Long = 0,
    val triggerDescriptions: List<String> = emptyList(), val nextRunAt: Long? = null,
    val timeoutSeconds: Int? = null, val lastError: String? = null,
) {
    val keepsOriginal: Boolean get() = triggers.size != 1 || triggers.single()["type"]?.jsonPrimitive?.content !in listOf("cron", "webhook")
    val hasWebhook: Boolean get() = triggers.any { it["type"]?.jsonPrimitive?.content in listOf("webhook", "event") }
}

@Serializable data class RoutineRun(
    val id: String, val routineId: String, val botId: String, val status: String,
    val createdAt: Long = 0, val finishedAt: Long? = null, val detail: String? = null,
    /** A retry of a run that failed before acting waits until then. */
    val notBefore: Long? = null,
) {
    val isActive: Boolean get() = status in listOf("pending", "starting", "running", "recovering")
    val label: String get() = when (status) {
        "pending" -> notBefore?.let { retrying(it) } ?: "Queued"; "starting" -> "Starting"; "running" -> "Running"
        "recovering" -> "Resuming after restart"; "failed" -> "Failed"; "interrupted" -> "Interrupted"
        else -> status.replaceFirstChar { it.uppercase() }
    }

    /** A retry waiting until `at` (Unix ms). */
    private fun retrying(at: Long): String {
        if (at <= System.currentTimeMillis()) return "Retrying"
        val time = Instant.ofEpochMilli(at).atZone(ZoneId.systemDefault()).toLocalTime()
        return "Retrying at " + time.format(DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT))
    }
}

@Serializable data class RoutineListing(val routines: List<Routine>, val runs: List<RoutineRun>) {
    fun summary(routine: Routine): String {
        runs.firstOrNull { it.routineId == routine.id && it.isActive }?.let { return it.label }
        if (routine.lastError != null) return "Schedule needs attention"
        runs.firstOrNull { it.routineId == routine.id }?.takeIf { it.status in listOf("failed", "interrupted") }
            ?.let { return "Last run ${it.label.lowercase()}" }
        return routine.triggerDescriptions.joinToString(" · ")
    }
}

/**
 * Whether the editor for `id` ("" sets up a new routine) can open: a routine's waits until it's listed,
 * since a chat link can name one before the first load (or the next poll) and an empty form would save
 * defaults over it.
 */
fun RoutineListing?.canEdit(id: String): Boolean = id.isEmpty() || this?.routines.orEmpty().any { it.id == id }

@Serializable data class RoutineScheduleDraft(
    val kind: String = "cron", val amount: String = "1", val unit: Long = 3600,
    val calendarStyle: String = "daily", val weekday: Int = 1, val selectedDays: List<Int> = emptyList(),
    val monthDay: Int = 1, val minuteStep: Int = 15, val hour: Int = 9, val minute: Int = 0,
    val at: Long = 0, val expression: String = "", val zone: String = "", val original: List<JsonObject> = emptyList(),
) {
    /** The phone edits the `routineRepeats` styles; the host's others open as Custom (the expression already holds their cron). */
    fun forMobileEditor(): RoutineScheduleDraft = copy(calendarStyle = calendarStyle.takeIf { style -> routineRepeats.any { it.first == style } } ?: "custom",
        kind = if (kind in listOf("cron", "webhook")) kind else "keep")

    /**
     * Switches the repeat style; Custom starts from `compiled`, the cron the host made of the current choice,
     * empty while that check is still out (never the cron the routine opened with).
     */
    fun repeating(style: String, compiled: String?): RoutineScheduleDraft = when {
        style == calendarStyle -> this
        style == "custom" -> copy(calendarStyle = style, expression = compiled.orEmpty())
        else -> copy(calendarStyle = style)
    }
}

/** The repeat styles the phone offers for a schedule, in menu order. */
val routineRepeats = listOf("hourly" to "Hourly", "daily" to "Daily", "weekdays" to "Weekdays", "weekly" to "Weekly", "custom" to "Custom")

@Serializable data class RoutineSchedulePreview(
    val draft: RoutineScheduleDraft, val triggers: List<JsonObject>, val summary: String,
    val nextRunAt: Long? = null, val warning: String? = null,
) {
    /** The cron the host compiled, when the schedule is one. */
    val cronExpression: String? get() = triggers.firstOrNull()?.takeIf { it["type"]?.jsonPrimitive?.content == "cron" }
        ?.get("expression")?.jsonPrimitive?.content
}

@Serializable data class RoutineWebhook(val url: String? = null, val localUrl: String, val key: String, val connected: Boolean)

data class RoutineDraft(val id: String? = null, val name: String = "", val instruction: String = "",
    val schedule: RoutineScheduleDraft = RoutineScheduleDraft(), val timeoutSeconds: String = "3600") {
    /** The timeout in words: whole hours, else whole minutes, else seconds; anything else as typed. */
    val timeoutText: String get() {
        val seconds = timeoutSeconds.toLongOrNull() ?: return "${timeoutSeconds}s"
        val (amount, unit) = when {
            seconds > 0 && seconds % 3600 == 0L -> seconds / 3600 to "hour"
            seconds > 0 && seconds % 60 == 0L -> seconds / 60 to "minute"
            else -> seconds to "second"
        }
        return "$amount $unit${if (amount == 1L) "" else "s"}"
    }

    fun body(botId: String): JsonObject = buildJsonObject {
        put("botId", botId); id?.let { put("id", it) }; put("name", name); put("instruction", instruction)
        put("schedule", WireJson.encodeToJsonElement(schedule)); put("timeoutSeconds", timeoutSeconds)
    }
}
