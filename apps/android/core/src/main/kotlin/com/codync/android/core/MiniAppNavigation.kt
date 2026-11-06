package com.codync.android.core

import java.net.URI

enum class MiniAppNavigation { InApp, Browser, Blocked }

/** The virtual site is one HTTPS origin, including its default port; lookalike hosts never qualify. */
fun miniAppHasVirtualOrigin(url: String): Boolean = parseMiniAppUri(url)?.let(::virtualOrigin) == true

fun miniAppNavigation(address: MiniAppAddress, url: String): MiniAppNavigation {
    val uri = parseMiniAppUri(url) ?: return MiniAppNavigation.Blocked
    return when {
        virtualOrigin(uri) -> if (appPage(address, uri)) MiniAppNavigation.InApp else MiniAppNavigation.Blocked
        uri.scheme?.lowercase() in setOf("http", "https") && !uri.host.isNullOrBlank() && uri.userInfo == null -> MiniAppNavigation.Browser
        else -> MiniAppNavigation.Blocked
    }
}

/** A native listener verifies the source frame; a page change also invalidates replies already in flight. */
class MiniAppPageGate(private val address: MiniAppAddress) {
    private var generation = 0L
    private var page: String? = null

    fun started(url: String?) {
        generation += 1
        page = url
    }

    fun admit(sourceOrigin: String, isMainFrame: Boolean, currentPage: String?): Long? {
        val source = parseMiniAppUri(sourceOrigin) ?: return null
        if (!isMainFrame || !virtualOrigin(source) || source.path.orEmpty() !in setOf("", "/") ||
            source.query != null || source.fragment != null || !currentPageAllowed(currentPage)) return null
        return generation
    }

    fun mayReply(ticket: Long, currentPage: String?): Boolean =
        ticket == generation && currentPageAllowed(currentPage)

    private fun currentPageAllowed(currentPage: String?): Boolean =
        page?.let { miniAppNavigation(address, it) == MiniAppNavigation.InApp } == true &&
            currentPage?.let { miniAppNavigation(address, it) == MiniAppNavigation.InApp } == true
}

private fun parseMiniAppUri(url: String): URI? = runCatching { URI(url) }.getOrNull()

private fun virtualOrigin(uri: URI): Boolean =
    uri.scheme.equals("https", ignoreCase = true) && uri.host.equals("codync-app.invalid", ignoreCase = true) &&
        uri.port in setOf(-1, 443) && uri.userInfo == null

private fun appPage(address: MiniAppAddress, uri: URI): Boolean {
    val path = uri.path.orEmpty()
    if ('\\' in path || path.split('/').any { it == "." || it == ".." }) return false
    return address.relativePath(path) != null
}
