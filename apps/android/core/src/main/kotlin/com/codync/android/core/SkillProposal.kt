package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.jsonObject

/** A skill a bot drafted after a procedure that used several kinds of tool (`skillProposals`). It is not a skill until approved. */
@Serializable data class SkillProposal(
    val id: String,
    val botId: String = "",
    val name: String,
    /** When to use it. */
    val description: String = "",
    /** The skill in Markdown. */
    val instructions: String = "",
    /** The kinds of tool the procedure used. */
    val tools: List<String> = emptyList(),
) {
    /** The first numbered steps, for the card. */
    fun stepsPreview(max: Int = 3): List<String> = instructions.lines().filter { STEP.matches(it) }.take(max)

    /** All three parts are there, so it can be approved. */
    val approvable: Boolean get() = name.isNotBlank() && description.isNotBlank() && instructions.isNotBlank()

    /** `execute:cargo` reads as the program. */
    fun toolsText(): String = tools.joinToString(", ") { if (it.startsWith("execute:")) it.removePrefix("execute:").ifEmpty { "commands" } else it }

    companion object {
        private val STEP = Regex("^\d+\.\s.*")

        fun decodeAll(response: JsonElement): List<SkillProposal> =
            WireJson.decodeFromJsonElement(kotlinx.serialization.builtins.ListSerializer(serializer()), response.jsonObject.getValue("proposals"))

        fun inboxTitle(count: Int) = if (count > 0) "Proposed skills ($count)" else "Proposed skills"
    }
}
