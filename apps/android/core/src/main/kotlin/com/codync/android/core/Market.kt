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

/** Registry metadata stays in memory. The Extensions tabs page through it. */
data class CatalogPage<T>(val items: List<T> = emptyList()) {
    fun append(page: List<T>, id: (T) -> String): CatalogPage<T> {
        val known = items.mapTo(mutableSetOf(), id)
        return copy(items = items + page.filter { known.add(id(it)) })
    }
}

/** How many catalog rows one Extensions page shows, and which page a click lands on. */
object CatalogPaging {
    const val pageSize = 10
    /** Two agent tiles across, three rows: a page has no leftover card. */
    const val phonePageSize = 6
    val pageSizes = listOf(6, 10, 20, 50)

    /** Loaded rows split into pages. While the registry still has a cursor, one more page is offered so the bar is never 1 / 1 for a list that continues. */
    fun pageCount(count: Int, size: Int = pageSize, hasMore: Boolean = false): Int {
        val loaded = if (count <= 0) 1 else (count + size - 1) / size
        return if (hasMore) loaded + 1 else loaded
    }

    fun clamp(page: Int, count: Int, size: Int = pageSize, hasMore: Boolean = false): Int =
        page.coerceIn(0, pageCount(count, size, hasMore) - 1)

    fun <T> slice(items: List<T>, page: Int, size: Int = pageSize): List<T> {
        val start = page.coerceAtLeast(0) * size
        return items.drop(start).take(size)
    }

    /** Next stays available through the last shown page, including the page that still has to be fetched. */
    fun canForward(count: Int, page: Int, hasMore: Boolean, size: Int = pageSize): Boolean =
        page < pageCount(count, size, hasMore) - 1

    /** A later page that isn't full yet, so the registry has to be asked. The first page is whatever the first answer held. */
    fun needsFetch(count: Int, page: Int, size: Int = pageSize): Boolean {
        val start = page.coerceAtLeast(0) * size
        return start > 0 && start + size > count
    }
}
