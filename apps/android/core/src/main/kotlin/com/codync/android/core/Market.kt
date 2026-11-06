package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.SerialName

@Serializable data class InstalledConnector(val id: String, val name: String, val description: String = "",
    val registryName: String? = null, val kind: String, val command: String? = null, val url: String? = null,
    val keys: List<String> = emptyList(), val logo: String? = null, val auth: String? = null, val lastFailure: ConnectorFailure? = null) {
    val needsSignIn: Boolean get() = auth == "signedOut"
}
@Serializable data class InstalledSkill(val id: String, val name: String, val description: String = "", val source: String, val path: String)
data class PluginListing(val connectors: List<InstalledConnector>, val skills: List<InstalledSkill>)
@Serializable data class MarketConnector(val name: String, val title: String, val description: String? = null,
    val version: String? = null, val website: String? = null, val installed: Boolean = false, val options: List<InstallOption>)
@Serializable data class InstallOption(val id: String, val kind: String, val label: String, val inputs: List<MarketInput> = emptyList()) {
    fun complete(values: Map<String, String>): Boolean = inputs.none { it.required && (values[it.name] ?: it.defaultValue.orEmpty()).isBlank() }
}
@Serializable data class MarketInput(val name: String, val description: String? = null, val secret: Boolean = false,
    val required: Boolean = false, @SerialName("default") val defaultValue: String? = null, val placeholder: String? = null)
@Serializable data class MarketConnectors(val items: List<MarketConnector>, val nextCursor: String? = null)
@Serializable data class MarketSkill(val source: String, val name: String, val description: String = "", val installed: Boolean = false, val category: String? = null, val creator: String? = null, val featured: Boolean = false, val pack: String? = null)

/** Registry metadata stays in memory. The Extensions tabs scroll through it. */
data class CatalogPage<T>(val items: List<T> = emptyList()) {
    fun append(page: List<T>, id: (T) -> String): CatalogPage<T> {
        val known = items.mapTo(mutableSetOf(), id)
        return copy(items = items + page.filter { known.add(id(it)) })
    }
}
@Serializable data class MarketAgent(val id: String, val name: String, val title: String, val category: String,
    val creator: String, val version: Int, val avatarColor: String, val avatarShape: String,
    val featured: Boolean = false, val installed: Boolean = false)
@Serializable data class MarketRoutineSchedule(val kind: String, val time: String? = null, val weekday: Int? = null, val day: Int? = null)
@Serializable data class MarketAgentSkill(val id: String, val name: String, val description: String = "", val pack: Boolean = false)
@Serializable data class MarketAgentRoutine(val name: String, val instruction: String = "", val active: Boolean = false,
    val schedule: MarketRoutineSchedule)
@Serializable data class MarketAgentDetail(val id: String, val name: String, val title: String, val category: String,
    val creator: String, val version: Int, val avatarColor: String, val avatarShape: String, val featured: Boolean = false,
    val installed: Boolean = false, val instructions: String = "", val skills: List<MarketAgentSkill> = emptyList(),
    val routines: List<MarketAgentRoutine> = emptyList())
@Serializable data class MarketAgentPage(val items: List<MarketAgent>)
@Serializable data class MarketAppField(val id: String, val label: String, val env: String? = null, val header: String? = null,
    val url: Boolean = false, val placeholder: String? = null, val hint: String? = null, val required: Boolean = false)
@Serializable data class MarketAppServer(val name: String, val transport: String, val url: String? = null,
    val command: String? = null, val args: List<String> = emptyList())
@Serializable data class MarketAppPrompt(val id: String, val text: String)
@Serializable data class MarketApp(val id: String, val name: String, val tagline: String, val description: String = "",
    val category: String, val creator: String, val version: String, val icon: String? = null, val website: String? = null,
    val featured: Boolean = false, val setup: String, val server: MarketAppServer, val fields: List<MarketAppField> = emptyList(),
    val steps: List<String> = emptyList(), val docsUrl: String? = null, val docsLabel: String? = null,
    val prompts: List<MarketAppPrompt> = emptyList(), val installed: Boolean = false)
@Serializable data class MarketAppPage(val items: List<MarketApp>)
