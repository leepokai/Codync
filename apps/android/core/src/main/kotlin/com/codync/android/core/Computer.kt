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
    /** Whether this computer sends push alerts to this phone, switched on its card. */
    val notifications: Boolean = true,
) {
    /** Keeps what this phone picked (color, route, push) over a newer copy from pairing or the account (iOS `keepDevicePicks`). */
    fun keepingDevicePicks(previous: Computer?): Computer =
        previous?.let { copy(color = it.color, route = it.route, notifications = it.notifications) } ?: this

    fun validate() {
        require(RelayCrypto.computerId(signKey.decodeBase64Url(32)) == id) { "This computer's identity changed. Pair it again." }
        boxKey?.decodeBase64Url(32)
        require(urls.size <= MAX_URLS && urls.all(::isDirectUrl)) { "Invalid computer address" }
        require(cloud == null || isCloudUrl(cloud)) { "Invalid cloud address" }
    }
}

/** The computer Usage follows, in the app and its widget: the last active one while it's still here, else the first. */
fun Iterable<Computer>.currentId(last: String?): String? = last?.takeIf { id -> any { it.id == id } } ?: firstOrNull()?.id

@Serializable
enum class ConnectionRoute { Automatic, CloudflareFirst, DirectOnly }

/** Phones keep at most this many direct addresses; a computer with many networks sends more, best first. */
internal const val MAX_URLS = 16

/** The direct addresses kept from a computer's list: valid ones, once each, the best [MAX_URLS]. */
internal fun directUrls(urls: Iterable<String>): List<String> = urls.filter(::isDirectUrl).distinct().take(MAX_URLS)

internal fun isDirectUrl(value: String): Boolean {
    val url = value.toHttpUrlOrNull() ?: return false
    return url.username.isEmpty() && url.password.isEmpty() && url.fragment == null
}

/** Debug builds set this: plain http to this machine also counts as a cloud (a local Wrangler), as in the iPhone's debug build. */
object LocalCloud {
    @Volatile var allowed = false
}

internal fun isCloudUrl(value: String): Boolean {
    val url = value.toHttpUrlOrNull() ?: return false
    val local = LocalCloud.allowed && url.scheme == "http" && url.host in setOf("127.0.0.1", "localhost")
    return (url.isHttps || local) && url.username.isEmpty() && url.password.isEmpty() && url.query == null && url.fragment == null
}

