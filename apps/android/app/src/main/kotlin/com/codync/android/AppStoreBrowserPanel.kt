package com.codync.android

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.add
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonObject

/** The computer's browser panel: its tabs and a picture of the page the host shows. */
internal suspend fun AppStore.browserPanel(): JsonObject = hostQuery("browserPanel", buildJsonObject {}).jsonObject

internal suspend fun AppStore.browserOpen(url: String): JsonObject =
    hostQuery("browserPanelOpen", buildJsonObject { put("url", url) }).jsonObject

internal suspend fun AppStore.browserActivate(tabId: String): JsonObject =
    hostQuery("browserPanelActivate", buildJsonObject { put("tabId", tabId) }).jsonObject

internal suspend fun AppStore.browserNavigate(tabId: String, url: String): JsonObject =
    hostQuery("browserPanelNavigate", buildJsonObject { put("tabId", tabId); put("url", url) }).jsonObject

internal suspend fun AppStore.browserClose(tabId: String): JsonObject =
    hostQuery("browserPanelClose", buildJsonObject { put("tabId", tabId) }).jsonObject

internal suspend fun AppStore.browserShutdown(): JsonObject = hostQuery("browserPanelShutdown", buildJsonObject {}).jsonObject

/** A PNG of the tab, base64, or null while it is loading. */
internal suspend fun AppStore.browserShot(tabId: String): String? =
    hostQuery("browserPanelShot", buildJsonObject { put("tabId", tabId) })
        .jsonObject["image"]?.jsonPrimitive?.content

/** Only kept for completeness: a tab list is decoded where it is shown. */
internal fun JsonObject.browserTabs(): List<Pair<String, String>> =
    (this["tabs"] as? kotlinx.serialization.json.JsonArray).orEmpty().mapNotNull { tab ->
        val object_ = tab.jsonObject
        val id = object_["id"]?.jsonPrimitive?.content ?: return@mapNotNull null
        val title = object_["title"]?.jsonPrimitive?.content?.takeIf(String::isNotBlank)
            ?: object_["url"]?.jsonPrimitive?.content.orEmpty()
        id to title
    }

internal fun tabJson(id: String, url: String): JsonObject = buildJsonObject {
    putJsonObject("schedule") {}
    put("id", id)
    put("url", url)
}

internal fun tabsJson(tabs: List<Pair<String, String>>) = buildJsonArray { tabs.forEach { (id, title) -> add("$id=$title") } }
