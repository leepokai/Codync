package com.codync.android.core

import kotlinx.serialization.Serializable

/** A saved setting that didn't take: `not_offered` (left out, never sent) or `rejected` (the agent refused it), and the value it was about. */
@Serializable data class SettingIssue(val issue: String = "", val value: String = "")

/** What the running agent session says about its settings, for the step of the chain it runs. */
data class SessionReport(val live: Boolean, val options: Map<String, String>?, val settings: List<ModelOption>?,
    val issues: Map<String, SettingIssue>, val pending: Boolean)

/** What agent [backend] says, when the bot runs on it now: its own agent unless it's on a fallback. Another agent's session says nothing about it. */
fun Bot.report(backend: String): SessionReport? =
    if ((agentStep?.agent ?: this.backend) != backend) null
    else SessionReport(sessionLive, sessionOptions, sessionSettings, settingIssues, restartPending)

/** The Effort level row of one step of a bot's chain (its own agent or a fallback). */
sealed interface EffortLevel {
    /** No row: the agent has no effort level for this model, as far as anyone knows, and nothing is saved. */
    data object None : EffortLevel
    /** Unknown until the agent runs this model. */
    data object Later : EffortLevel
    /** The setting to pick from (when known) and the one line under it. */
    data class Row(val setting: ModelOption?, val caption: String?) : EffortLevel

    companion object {
        /** The line under a fallback's row, without starting its agent: its effort level as its own running session reports it, or else what
         *  is saved for it (Edit loads the agent's list to change it). */
        fun fallbackLine(saved: Map<String, String>, report: SessionReport?): String? {
            val found = report?.settings?.let(::setting)
            if (found != null) {
                val row = of(listOf(found), saved[found.id], report, loading = false, failed = false, chosenModel = false)
                if (row is Row && row.caption != null) return "Effort level · ${row.caption}"
            }
            val values = saved.toSortedMap().values
            return if (values.isEmpty()) null else "Saved: ${values.joinToString(", ")}"
        }

        /** The agent's effort level: the setting of ACP category `thought_level`, whatever its id. */
        fun setting(settings: List<ModelOption>): ModelOption? = settings.firstOrNull { it.category == "thought_level" }

        /** [catalog]: what the agent lists for the step's model; [saved]: what the host saved; [report]: the step's own agent's session while it runs.
         *  First match wins; nothing is inferred from a saved value. */
        fun of(catalog: List<ModelOption>, saved: String?, report: SessionReport?, loading: Boolean, failed: Boolean, chosenModel: Boolean): EffortLevel {
            // While nothing is pending, the running session's own list is for the model it runs.
            val listed = report?.takeIf { !it.pending }?.settings?.let(::setting)
            val found = listed ?: setting(catalog)
            if (found == null) {
                return when {
                    loading -> saved?.let { Row(null, "Saved: $it · checking this agent’s options") } ?: None
                    failed -> saved?.let { Row(null, "Saved: $it · couldn’t check this agent’s options") } ?: None
                    chosenModel && catalog.isEmpty() -> Later
                    else -> saved?.let { Row(null, "Saved: $it · not offered for this model") } ?: None
                }
            }
            fun name(v: String) = found.values.firstOrNull { it.id == v }?.name ?: v
            val reported = report?.options?.get(found.id)
            val issue = report?.issues?.get(found.id)?.takeIf { saved != null && it.value == saved }
            val caption = when {
                saved != null && issue?.issue == "rejected" -> "Saved: ${name(saved)} · not applied (the agent refused it)"
                saved != null && issue != null ->
                    if (reported != null) "In use: ${name(reported)} · Saved: ${name(saved)} not offered" else "Saved: ${name(saved)} · not offered for this model"
                report?.pending == true && saved != reported -> {
                    val next = saved?.let(::name) ?: "Default"
                    if (reported != null) "In use: ${name(reported)} · $next pending" else "Current: unknown · $next pending"
                }
                report?.live != true -> saved?.let { "Not running · Saved: ${name(it)}" } ?: "Not running"
                reported == null -> "Current: unknown"
                saved == null -> "In use: ${name(reported)} (agent default)"
                saved == reported -> "In use: ${name(reported)}"
                else -> "Saved: ${name(saved)} · not confirmed (in use: ${name(reported)})"
            }
            return Row(found, caption)
        }
    }
}
