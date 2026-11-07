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
    /** What a due occurrence that could not run follows: `skip` or `runOnce` (older hosts send nothing). */
    val missedPolicy: String? = null,
    /** What a run that would pass the bot's daily token limit follows: `wait` or `skip` (older hosts send nothing). */
    val limitPolicy: String? = null,
    /** Where a workflow's last reply goes (older hosts send nothing). */
    val outputs: List<RoutineOutput> = emptyList(),
    /** Its runs post in a thread of the bot chat. */
    val thread: Boolean = false,
) {
    val keepsOriginal: Boolean get() = triggers.size != 1 || triggers.single()["type"]?.jsonPrimitive?.content !in listOf("cron", "interval", "event", "webhook")
    val hasWebhook: Boolean get() = triggers.any { it["type"]?.jsonPrimitive?.content in listOf("webhook", "event") }
}

/** Occurrences that came due and did not run; `count` is how many. */
@Serializable data class RoutineRunMissed(val count: Int, val until: Long? = null)

@Serializable data class RoutineRun(
    val id: String, val routineId: String, val botId: String, val status: String,
    val createdAt: Long = 0, val finishedAt: Long? = null, val detail: String? = null,
    /** A retry of a run that failed before acting waits until then. */
    val notBefore: Long? = null,
    /** The occurrence this run stands for, when its schedule named one. */
    val scheduledFor: Long? = null,
    /** Occurrences that came due and did not run (older hosts send nothing). */
    val missed: RoutineRunMissed? = null,
    /** A run holds the bot's turn and it is asking a question: it waits on the user. */
    val needsAttention: Boolean? = null,
) {
    val isActive: Boolean get() = status in listOf("pending", "starting", "running", "recovering")
    val label: String get() {
        val missedSuffix = missed?.takeIf { it.count > 0 }?.let { " · ${it.count} missed" } ?: ""
        return when (status) {
            "pending" -> notBefore?.let { retrying(it) } ?: "Queued"
            "starting" -> "Starting"
            "running" -> if (needsAttention == true) "Waiting on you" else "Running"
            "recovering" -> "Resuming after restart"
            "failed" -> "Failed$missedSuffix"
            "interrupted" -> "Interrupted$missedSuffix"
            "cancelled" -> // A cancelled run that stands for a due occurrence is a skipped one; the rest just stopped.
                if (scheduledFor != null || missed != null) "Skipped$missedSuffix" else "Cancelled"
            else -> status.replaceFirstChar { it.uppercase() }
        }
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
        runs.firstOrNull { it.routineId == routine.id }?.let { last ->
            if (last.status in listOf("failed", "interrupted") || isSkipped(last)) return "Last run ${last.label.lowercase()}"
        }
        return routine.triggerDescriptions.joinToString(" · ")
    }

    /** A cancelled run that stands for a due occurrence is a skipped one; the rest just stopped. */
    private fun isSkipped(run: RoutineRun): Boolean = run.status == "cancelled" && (run.scheduledFor != null || run.missed != null)
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
    val monthDay: Int = 1,
    /** Yearly: the month (1-12) of the one date. */
    val yearMonth: Int = 1,
    val minuteStep: Int = 15,
    /** Hourly: repeat every N hours (1, 2, 3, 4, 6, 8, 12); 1 runs on the hour. */
    val everyHours: Int = 1,
    /** Hourly: the window, in minutes from midnight; `null` on either end runs through the day. */
    val windowStart: Int? = null, val windowEnd: Int? = null,
    val hour: Int = 9, val minute: Int = 0,
    val at: Long = 0, val expression: String = "", val zone: String = "", val original: List<JsonObject> = emptyList(),
    /** An event trigger: the service, the event it sends and its filters. */
    val eventSource: String = "github", val eventName: String = "issues.opened",
    val eventFilters: Map<String, String> = emptyMap(),
) {
    /**
     * What the editor shows: the host's own reading of the trigger. Kinds the editor can't edit (`once`,
     * several triggers) keep the original triggers; the visual style the host detected is retained, never forced to `custom`.
     */
    fun forMobileEditor(): RoutineScheduleDraft = copy(kind = if (kind in RoutineSchedule.editableKinds) kind else "keep")


    /** Toggle one weekday (0 Sunday ... 6 Saturday) of the `days` style; the host rejects an empty set. */
    fun toggleDay(day: Int): RoutineScheduleDraft =
        copy(selectedDays = (if (day in selectedDays) selectedDays - day else selectedDays + day).sorted())
}

@Serializable data class RoutineSchedulePreview(
    val draft: RoutineScheduleDraft, val triggers: List<JsonObject>, val summary: String,
    val nextRunAt: Long? = null, val nextRuns: List<Long>? = null, val warning: String? = null,
    /** The event services and filters the editor offers. */
    val sources: List<RoutineEventSource> = emptyList(),
)

/** A service a routine can run on, its events and the filters it takes (host `routines::events`). */
@Serializable data class RoutineEventSource(val id: String, val label: String, val events: List<RoutineEvent>, val filters: List<RoutineEventFilter>)

@Serializable data class RoutineEvent(val id: String, val label: String)

@Serializable data class RoutineEventFilter(val key: String, val label: String, val hint: String)

@Serializable data class RoutineWebhook(val url: String? = null, val localUrl: String, val key: String, val connected: Boolean)

/** Where a workflow's last reply goes: a channel, a chat, or the issue that started it. */
@Serializable data class RoutineOutput(
    val kind: String = "slack",
    val target: String = "",
    val account: String? = null,
    /** Labels, reviewers, or the issue title. */
    val value: String? = null,
)

/** The output kinds the editor offers, and their destination rules (host `routines::outputs`). */
object RoutineOutputs {
    val kinds = listOf(
        "slack" to "Slack", "telegram" to "Telegram", "discord" to "Discord", "discordReact" to "Discord reaction",
        "githubComment" to "GitHub comment", "githubIssue" to "GitHub issue", "githubLabel" to "GitHub label",
        "githubAssign" to "GitHub assign", "githubReview" to "Request review",
    )

    fun problem(output: RoutineOutput): String? {
        val target = output.target.trim()
        if (target.isEmpty()) return "An output needs a destination."
        if (output.kind == "telegram" && target.toLongOrNull() == null) return "Use a Telegram chat id."
        if (output.kind in listOf("githubComment", "githubIssue", "githubLabel", "githubAssign", "githubReview")
            && !Regex("^[^/\\s]+/[^/\\s]+$").matches(target)) return "Use owner/name."
        if (output.kind in listOf("githubLabel", "githubAssign", "githubReview") && output.value.isNullOrBlank()) return "This output needs a value."
        if (output.kind in listOf("slack", "discord") && target.any(Char::isWhitespace)) return "Use a channel id."
        if (output.kind == "discordReact" && target.any(Char::isWhitespace)) return "Use one emoji or name:id."
        return null
    }
}

data class RoutineDraft(val id: String? = null, val name: String = "", val instruction: String = "",
    val schedule: RoutineScheduleDraft = RoutineScheduleDraft(), val timeoutSeconds: String = "3600",
    /** Absent (null) keeps the saved policies, the host's default on a new routine. */
    val missedPolicy: String? = null, val limitPolicy: String? = null, val outputs: List<RoutineOutput> = emptyList(), val thread: Boolean = false) {
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
        missedPolicy?.let { put("missedPolicy", it) }; limitPolicy?.let { put("limitPolicy", it) }
        put("outputs", WireJson.encodeToJsonElement(outputs))
        put("thread", thread)
    }
}
