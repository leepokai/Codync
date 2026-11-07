package com.codync.android

import com.codync.android.core.WireJson
import kotlinx.serialization.Serializable

/** The saved browser proxy of a bot: shown masked, never with its password. */
@Serializable data class BrowserProxy(val proxy: String? = null, val authenticated: Boolean = false)

suspend fun AppStore.browserProxy(botId: String): BrowserProxy =
    WireJson.decodeFromJsonElement(BrowserProxy.serializer(), hostQuery("browserProxy", body("botId" to botId)))

suspend fun AppStore.setBrowserProxy(botId: String, address: String): BrowserProxy =
    WireJson.decodeFromJsonElement(BrowserProxy.serializer(), hostQuery("setBrowserProxy", body("botId" to botId, "address" to address)))

suspend fun AppStore.clearBrowserProxy(botId: String): BrowserProxy =
    WireJson.decodeFromJsonElement(BrowserProxy.serializer(), hostQuery("clearBrowserProxy", body("botId" to botId)))
