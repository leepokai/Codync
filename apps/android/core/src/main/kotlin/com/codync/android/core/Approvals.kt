package com.codync.android.core

import kotlinx.serialization.Serializable

/** One of the modes an agent works in (Claude's Manual / Plan / Auto, Codex's Read-only / Auto review…). */
@Serializable data class AgentMode(val id: String, val name: String, val description: String? = null) {
    companion object {
        /** Codync's id for the agent's Auto mode, whatever the agent calls it: new bots start there. */
        const val AUTO = "auto"
    }
}

/** A bot's Approvals choice: [note] is the agent's own words, untranslated; [detail] is Codync's, to translate. */
data class ApprovalChoice(val id: String, val label: String, val note: String?, val detail: String) {
    companion object {
        const val ASK = "ask"
        const val APPROVE = "approve"
        private const val ASKS = "Anything the agent still asks about comes to you as an approval card."
        private val askMe = ApprovalChoice(ASK, "Ask me", null, "You'll get an approval card and a notification whenever the agent asks.")
        private fun modeId(mode: String) = "mode:$mode"

        /**
         * The choices for [draft] on an agent with this model list (null while it loads), and the one its setting shows
         * as: the agent's modes, its Auto mode first, then Codync approving everything. An agent without modes offers
         * Ask me instead; while its list loads (or from an older host), Auto and Ask me. A bot without a mode (or in
         * Auto on an agent without one) shows as Ask me, and one in a mode the list lacks as that mode's id: never as
         * more careful than it is.
         */
        fun choices(draft: BotDraft, catalog: AgentModels?): Pair<List<ApprovalChoice>, ApprovalChoice> {
            val approve = ApprovalChoice(APPROVE, "Approve automatically", null,
                "Tool requests are approved automatically. Your agent's own settings (like Claude Code's permission rules) still apply.")
            val modes = catalog?.modes
            val all = when {
                modes == null -> mutableListOf(ApprovalChoice(modeId(AgentMode.AUTO), "Auto", null, "The agent decides what's safe and asks you about the rest."), askMe)
                modes.isEmpty() -> mutableListOf(askMe)
                else -> modes.mapTo(mutableListOf()) { ApprovalChoice(modeId(it.id), it.name, it.description, ASKS) }
            }
            all += approve
            if (draft.permission == "auto") return all to approve
            val mode = draft.mode
            // Unlisted: Auto on an agent without it is its default, like no mode; any other mode is itself.
            val unlisted = if (mode != null && mode != AgentMode.AUTO) ApprovalChoice(modeId(mode), mode, null, ASKS) else askMe
            val found = all.firstOrNull { it.id == (mode?.let(::modeId) ?: ASK) } ?: all.firstOrNull { it.id == unlisted.id }
            if (found != null) return all to found
            all.add(all.size - 1, unlisted)
            return all to unlisted
        }
    }
}

/** The draft with an Approvals choice picked. */
fun BotDraft.withApproval(id: String): BotDraft = when (id) {
    ApprovalChoice.APPROVE -> copy(permission = "auto", mode = null)
    ApprovalChoice.ASK -> copy(permission = "ask", mode = null)
    else -> copy(permission = "ask", mode = id.removePrefix("mode:"))
}
