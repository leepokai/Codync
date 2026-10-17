package com.codync.android.core

import kotlinx.serialization.Serializable
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

@Serializable
data class Computer(
    val id: String,
    val name: String,
    val signKey: String,
    val boxKey: String? = null,
    val urls: List<String> = emptyList(),
    val cloud: String? = null,
    val route: ConnectionRoute = ConnectionRoute.Automatic,
    /** The badge color picked on this phone (an `AvatarPalette` id). */
    val color: String? = null,
) {
    fun validate() {
        require(RelayCrypto.computerId(signKey.decodeBase64Url(32)) == id) { "This computer's identity changed. Pair it again." }
        boxKey?.decodeBase64Url(32)
        require(urls.size <= 16 && urls.all(::isDirectUrl)) { "Invalid computer address" }
        require(cloud == null || isCloudUrl(cloud)) { "Invalid cloud address" }
    }
}

@Serializable
enum class ConnectionRoute { Automatic, CloudflareFirst, DirectOnly }

internal fun isDirectUrl(value: String): Boolean {
    val url = value.toHttpUrlOrNull() ?: return false
    return url.username.isEmpty() && url.password.isEmpty() && url.fragment == null
}

internal fun isCloudUrl(value: String): Boolean {
    val url = value.toHttpUrlOrNull() ?: return false
    return url.isHttps && url.username.isEmpty() && url.password.isEmpty() && url.query == null && url.fragment == null
}

