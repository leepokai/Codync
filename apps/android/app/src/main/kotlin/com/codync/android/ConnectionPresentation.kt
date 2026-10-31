package com.codync.android

import com.codync.android.core.CloudComputer
import com.codync.android.core.Computer
import com.codync.android.core.ConnectionRoute
import com.codync.android.core.HostRoute
import com.codync.android.core.LinkState
import com.codync.android.core.Pairing

// What the roster header, Computer management and the chat header say about the computer's link
// (iOS `ComputerStatus`, `ConnectionSummary`, `BotStore.connectionLabel`).

/** Turned away by the computer: access revoked, or its identity changed. */
private val LinkState.unauthorized get() = this is LinkState.Failed && status == 403

/** One short line for the computer's connection, next to its name (iOS `BotStore.statusText`). */
internal fun statusText(state: AppState): String = if (state.isUpdating) "Updating…" else when (val link = state.connection) {
    is LinkState.Ready -> if (state.mismatch != null) "Needs update" else "Online"
    LinkState.Connecting -> "Connecting…"
    is LinkState.ComputerOffline -> link.lastSeenAt?.let { "Offline · seen ${RelativeTime.day(it)}" } ?: "Offline"
    is LinkState.Failed -> if (link.unauthorized) link.message else "Can't reach"
}

/** The status line in Computer management: one that's offline while its new identity is around says so. */
internal fun computerStatusText(state: AppState, stale: Boolean): String =
    if (stale && !state.connection.unauthorized) "Older copy of this computer · ${statusText(state)}" else statusText(state)

/** A word for the link: the computer filter's rows and the chat header. */
internal fun connectionLabel(state: AppState): String = when (val link = state.connection) {
    is LinkState.Ready -> if (state.mismatch != null) "Needs update" else "Connected"
    LinkState.Connecting -> "Connecting…"
    is LinkState.ComputerOffline -> "Offline"
    is LinkState.Failed -> if (link.unauthorized) "No access" else "Offline"
}

/** What TalkBack says for the chat header's connection: the route too, when there is one. */
internal fun connectionDescription(state: AppState): String {
    val link = state.connection
    return when {
        link == LinkState.Connecting -> "Connecting"
        link !is LinkState.Ready -> connectionLabel(state)
        state.mismatch != null -> "Needs update"
        link.route == HostRoute.Relay -> connectedThrough(state.computer.cloud)
        else -> "Connected over Wi-Fi or Tailscale"
    }
}

/** Host of a private relay, or null when this is an official Codync cloud (those stay "Cloudflare"). */
internal fun relayHost(cloud: String?): String? {
    if (cloud.isNullOrBlank()) return null
    val uri = runCatching { java.net.URI(cloud.trim()) }.getOrNull() ?: return null
    // `URI.host` keeps an IPv6 address's brackets.
    val name = uri.host?.lowercase() ?: return null
    if ((name == "api.codync.dev" || name == "dev-api.codync.dev") && uri.port == -1) return null
    return if (uri.port > 0) "$name:${uri.port}" else name
}

/** The route after a bot count in Computer management (iOS `RelayCopy.detailSuffix`). */
internal fun routeLabel(route: HostRoute, cloud: String?): String =
    if (route == HostRoute.Direct) "Wi-Fi/Tailscale" else relayHost(cloud)?.let { "Private relay ($it)" } ?: "Cloudflare"

internal fun connectedThrough(cloud: String?): String {
    val host = relayHost(cloud)
    return if (host == null) "Connected through Cloudflare" else "Connected through Private relay ($host)"
}

internal fun routeTitle(route: ConnectionRoute, cloud: String?): String {
    val host = relayHost(cloud) ?: return when (route) {
        ConnectionRoute.Automatic -> "Auto: Wi-Fi or Tailscale, then Cloudflare"
        ConnectionRoute.CloudflareFirst -> "Cloudflare, then Wi-Fi or Tailscale"
        ConnectionRoute.DirectOnly -> "Wi-Fi or Tailscale only, never Cloudflare"
    }
    return when (route) {
        ConnectionRoute.Automatic -> "Auto: Wi-Fi or Tailscale, then Private relay ($host)"
        ConnectionRoute.CloudflareFirst -> "Private relay ($host), then Wi-Fi or Tailscale"
        ConnectionRoute.DirectOnly -> "Wi-Fi or Tailscale only, never Private relay ($host)"
    }
}

/** The route's short name on the Connection row; the menu keeps `routeTitle` (iOS `RelayCopy.routeShortTitle`). */
internal fun routeShortTitle(route: ConnectionRoute, cloud: String?): String = when (route) {
    ConnectionRoute.Automatic -> "Auto"
    ConnectionRoute.CloudflareFirst -> if (relayHost(cloud) == null) "Cloudflare first" else "Private relay first"
    ConnectionRoute.DirectOnly -> "Wi-Fi/Tailscale only"
}

internal fun pairingLine(cloud: String?): String {
    val host = relayHost(cloud)
    return if (host == null) "Connects on Wi-Fi, over Tailscale, or from anywhere through Cloudflare, on its own. End-to-end encrypted."
    else "Connects on Wi-Fi, over Tailscale, or from anywhere through Private relay ($host), on its own. End-to-end encrypted."
}

/** The pairing page's line. Parsing stays here so a view only renders the sentence. */
internal fun pairingLine(pasted: String, scheme: String): String =
    pairingLine(runCatching { Pairing.parse(pasted, scheme).computer.cloud }.getOrNull())

/** A sentence to translate, and the counts that fill its `%d`s in order. */
internal data class Phrase(val template: String, val counts: List<Int> = emptyList()) {
    /** [translated] (the template in the app's language) with the counts filled in. */
    fun filled(translated: String = template): String = counts.fold(translated) { text, count -> text.replaceFirst("%d", "$count") }
}

/** "3 connected", "1/2 connected", "Connecting…", "No access": the roster header's summary of the computers it shows (iOS `ConnectionSummary`). */
internal fun connectionSummary(states: List<AppState>): Phrase {
    if (states.isEmpty()) return Phrase("Computers")
    val online = states.count { it.connection is LinkState.Ready }
    return when {
        online == states.size -> Phrase("%d connected", listOf(online))
        online > 0 -> Phrase("%d/%d connected", listOf(online, states.size))
        states.any { it.connection is LinkState.Connecting } -> Phrase("Connecting…")
        states.all { it.connection.unauthorized } -> Phrase("No access")
        else -> Phrase("%d offline", listOf(states.size))
    }
}

/**
 * A computer this phone uses that won't come back as it is: it turned this phone away, or it's offline
 * while a computer with its name that joined the account after it was last seen is online, the same
 * machine under a new identity (iOS `AccountStore.isStale`). Account computers offer access recovery; only independently paired computers offer removal.
 */
internal fun AccountContext.isStale(state: AppState, others: Collection<AppState>): Boolean {
    val computer = state.computer
    val seen = cloudComputers.firstOrNull { it.computerId == computer.id }?.lastSeenAt ?: (state.connection as? LinkState.ComputerOffline)?.lastSeenAt
    val renamedCopyOnline = others.any { it.computer.id != computer.id && it.computer.name == computer.name && it.connection is LinkState.Ready && replacedBy(it.computer.id, seen) }
        || cloudComputers.any { it.computerId != computer.id && it.name == computer.name && it.online == true && replacedBy(it.computerId, seen) }
    return when (val link = state.connection) {
        is LinkState.Failed -> link.unauthorized || renamedCopyOnline
        is LinkState.ComputerOffline -> renamedCopyOnline
        else -> false
    }
}

/**
 * A computer last [seen] no later than account computer [id] joined the account was replaced by it;
 * seen after, it ran beside it: another machine with the same name. Without both times the name decides.
 */
private fun AccountContext.replacedBy(id: String, seen: Long?): Boolean {
    seen ?: return true
    val claimed = cloudComputers.firstOrNull { it.computerId == id }?.claimedAt ?: return true
    return seen <= claimed
}

/**
 * An offline account computer named like one this phone uses or sees online, and [replacedBy] it: most
 * likely the same machine under an earlier identity (its host keys were reset), left behind in the account.
 */
internal fun AccountContext.isOlderCopy(computer: CloudComputer): Boolean {
    if (computer.online == true) return false
    val seen = computer.lastSeenAt
    // The current copy may be paired with its code without being listed in the account.
    return computers.any { it.id != computer.computerId && it.name == computer.name && replacedBy(it.id, seen) }
        || cloudComputers.any { other -> other.computerId != computer.computerId && other.name == computer.name
            && (computers.any { it.id == other.computerId } || other.online == true) && replacedBy(other.computerId, seen) }
}
