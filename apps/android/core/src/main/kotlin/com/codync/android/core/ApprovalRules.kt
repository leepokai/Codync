package com.codync.android.core

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.decodeFromJsonElement

/** A bot's rule for an agent's permission requests; the first that matches decides (the host's `approval_rules`). */
@Serializable data class ApprovalRule(val pattern: String, val kind: String? = null, val decision: String = "ask")

/**
 * What the host says a command would do under some rules (`testApprovalRule`). The host decides; this only words it.
 * [outcome] is `forbidden`, `rule` or `setting`.
 */
@Serializable data class ApprovalEvaluation(
    val outcome: String,
    val pattern: String? = null,
    val index: Int? = null,
    val decision: String? = null,
    val permission: String? = null,
    val kind: String? = null,
    val quick: Boolean = false,
    val chained: Boolean = false,
    val allowPassedOver: Int? = null,
) {
    /** One sentence; rule numbers count from 1, as the list shows them. */
    fun describe(): String {
        val passed = allowPassedOver?.let { " Rule ${it + 1} would allow it, but it runs several commands." }.orEmpty()
        return when (outcome) {
            "forbidden" -> "Never allowed: it matches “$pattern” in the list of forbidden commands, whatever the rules say."
            "rule" -> {
                val verb = when (decision) { "allow" -> "Allowed"; "deny" -> "Denied"; else -> "Asks you" }
                if (quick) "$verb by the quick rule for ${QUICK_NAMES[kind] ?: "this kind of action"}."
                else "$verb by rule ${(index ?: 0) + 1} (“$pattern”)."
            }
            else -> "No rule matches: " + (if (permission == "auto") "it is approved automatically" else "it asks you") +
                ", as the bot's Approvals setting says.$passed"
        }
    }
}

/** What a quick rule is called after the tool kind it covers. */
private val QUICK_NAMES = mapOf("read" to "files", "search" to "files", "edit" to "files", "delete" to "files", "move" to "files",
    "execute" to "shell commands", "fetch" to "web access")

@Serializable private data class EvaluationEnvelope(@SerialName("evaluation") val evaluation: ApprovalEvaluation)

object ApprovalRules {
    /** The quick rules ("by kind of action") approvals can enforce: wire class, label, and the tool kinds each covers. */
    val QUICK_CLASSES = listOf(
        Triple("files", "Files", "read, search, edit, delete, move"),
        Triple("shell", "Shell commands", "execute"),
        Triple("browse", "Web access", "fetch"),
    )
    val QUICK_POLICIES = listOf("" to "Not set", "allow" to "Allow", "ask" to "Ask me", "block" to "Deny")

    /** The tool kinds a rule can be limited to. */
    val KINDS = listOf("execute", "edit", "delete", "move", "read", "search", "fetch", "think", "other")
    val DECISIONS = listOf("allow" to "Allow", "ask" to "Ask me", "deny" to "Deny")
    const val MAX_RULES = 100
    const val MAX_PATTERN = 500

    /** What an `allow` rule never approves, shown beside the editor. */
    const val CHAINING_WARNING = "An allow rule never approves a command that runs several (; && || | ` \$( > < or a new line): such a command asks instead."

    /** The rules with the one at [from] moved to [to] (the first match decides, so order matters). */
    fun move(rules: List<ApprovalRule>, from: Int, to: Int): List<ApprovalRule> {
        if (from == to || from !in rules.indices || to !in rules.indices) return rules
        val next = rules.toMutableList()
        next.add(to, next.removeAt(from))
        return next
    }

    /** The rules a draft can save: blank patterns are dropped, patterns trimmed. */
    fun clean(rules: List<ApprovalRule>): List<ApprovalRule> =
        rules.map { it.copy(pattern = it.pattern.trim(), kind = it.kind?.takeIf(String::isNotEmpty)) }.filter { it.pattern.isNotEmpty() }

    fun decodeEvaluation(response: JsonElement): ApprovalEvaluation = WireJson.decodeFromJsonElement<EvaluationEnvelope>(response).evaluation
}
