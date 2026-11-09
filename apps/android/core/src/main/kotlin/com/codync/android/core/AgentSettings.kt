package com.codync.android.core

import kotlinx.serialization.Serializable

/** A model setting (effort, fast…) and the values the agent accepts. */
@Serializable data class ModelOption(val id: String, val name: String, val values: List<ModelOptionValue> = emptyList(), val default: String? = null, val kind: String? = null) {
    val isReasoning: Boolean get() = kind == "reasoning" || id in listOf("effort", "reasoning_effort", "reasoning-effort", "thought_level")
}
@Serializable data class ModelOptionValue(val id: String, val name: String)
@Serializable data class AgentModel(val id: String, val name: String, val description: String? = null, val options: List<ModelOption>? = null, val optionsError: String? = null)
/** `fastMode`: the agent offers Fast mode (absent on older hosts). `modes`: its modes, Auto first and listed as `auto`
 *  (absent on older hosts). */
@Serializable data class AgentModels(val models: List<AgentModel>, val currentModelId: String? = null, val fastMode: Boolean? = null, val defaultModelOptions: Boolean? = null,
    val modes: List<AgentMode>? = null) {
    val agentDefault: AgentModel? get() = models.firstOrNull { it.name.lowercase().startsWith("default") }
    fun options(selected: String?): List<ModelChoice> {
        val default = agentDefault
        val available = models.filter { it.id != default?.id }
        val current = available.firstOrNull { it.id == currentModelId }
        val choices = mutableListOf(ModelChoice(null, default?.name ?: current?.let { "Default · ${it.name}" } ?: "Default", default?.description))
        choices += available.map { ModelChoice(it.id, it.name, it.description) }
        if (!selected.isNullOrBlank() && selected != default?.id && available.none { it.id == selected }) choices += ModelChoice(selected, selected)
        return choices
    }
    fun normalize(selected: String?): String? = selected?.takeIf { it.isNotBlank() && it != agentDefault?.id }
}
data class ModelChoice(val id: String?, val name: String, val description: String? = null)

@Serializable data class RecentWorkspace(val path: String, val usedAt: Long)
@Serializable data class DirListing(val path: String, val parent: String? = null, val isGit: Boolean = false, val dirs: List<DirItem>,
    val recent: List<RecentWorkspace>? = null, val ancestors: List<DirItem>? = null)
@Serializable data class DirItem(val name: String, val path: String, val isGit: Boolean = false)
