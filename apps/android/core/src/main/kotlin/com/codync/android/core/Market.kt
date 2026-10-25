package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.SerialName

@Serializable data class InstalledConnector(val id: String, val name: String, val description: String = "",
    val registryName: String? = null, val kind: String, val command: String? = null, val url: String? = null,
    val keys: List<String> = emptyList(), val logo: String? = null, val auth: String? = null) {
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
@Serializable data class MarketSkill(val source: String, val name: String, val description: String = "", val installed: Boolean = false)

/** Registry metadata stays in memory. The Extensions tabs scroll through it. */
data class CatalogPage<T>(val items: List<T> = emptyList()) {
    fun append(page: List<T>, id: (T) -> String): CatalogPage<T> {
        val known = items.mapTo(mutableSetOf(), id)
        return copy(items = items + page.filter { known.add(id(it)) })
    }
}
