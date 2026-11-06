package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.jsonObject

/** One rule a profile writes: a tool kind and what happens to it. */
@Serializable data class ProfileRule(val kind: String? = null, val decision: String = "ask")

/**
 * A security profile the host offers (`securityProfiles`): read-only, builder, operator or full. The host owns the
 * meaning; this words it for the person choosing.
 */
@Serializable data class SecurityProfile(
    val id: String,
    val title: String,
    /** One sentence for the person choosing. */
    val summary: String,
    /** The Approvals setting the profile leaves: `ask` or `auto`. */
    val permission: String = "ask",
    /** The daily token limit it sets, when it sets one. */
    val budget: Long? = null,
    val rules: List<ProfileRule> = emptyList(),
) {
    /** The rules as the editor lists them: a `*` pattern per tool kind. */
    fun approvalRules(): List<ApprovalRule> = rules.map { ApprovalRule(pattern = "*", kind = it.kind, decision = it.decision) }

    /** The lines that say what choosing it sets: by decision the tool kinds, then the setting and the limit. */
    fun preview(): List<String> {
        val lines = mutableListOf<String>()
        for ((decision, verb) in listOf("allow" to "Allows", "ask" to "Asks before", "deny" to "Refuses")) {
            val kinds = rules.filter { it.decision == decision }.map { it.kind ?: "anything" }
            if (kinds.isNotEmpty()) lines += "$verb: ${kinds.joinToString(", ")}"
        }
        lines += if (permission == "auto") "Everything else: approved automatically" else "Everything else: asks you"
        budget?.let { lines += "Daily limit: ${"%,d".format(java.util.Locale.US, it)} tokens" }
        lines += "Commands no bot may run stay refused under every profile."
        return lines
    }

    companion object {
        fun decodeAll(response: JsonElement): List<SecurityProfile> =
            WireJson.decodeFromJsonElement(kotlinx.serialization.builtins.ListSerializer(serializer()), response.jsonObject.getValue("profiles"))
    }
}
