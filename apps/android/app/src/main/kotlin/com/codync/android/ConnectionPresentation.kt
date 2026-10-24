package com.codync.android

import com.codync.android.core.CloudComputer
import com.codync.android.core.Computer
import com.codync.android.core.HostRoute
import com.codync.android.core.LinkState

// What the roster header, Computers & settings and the chat header say about the computer's link
// (iOS `ComputerStatus`, `ConnectionSummary`, `BotStore.connectionLabel`).

/** Turned away by the computer: access revoked, or its identity changed. */
private val LinkState.unauthorized get() = this is LinkState.Failed && status == 403

/** One short line for the computer's connection, next to its name (iOS `BotStore.statusText`). */
internal fun statusText(state: AppState): String = if (state.computers.isEmpty()) "Not paired" else when (val link = state.connection) {
    is LinkState.Ready -> if (state.mismatch != null) "Needs update" else "Online"
    LinkState.Connecting -> "Connecting…"
    is LinkState.ComputerOffline -> link.lastSeenAt?.let { "Offline · seen ${RelativeTime.day(it)}" } ?: "Offline"
    is LinkState.Failed -> if (link.unauthorized) link.message else "Can't reach"
}

/** The status line in Computers & settings: one that's offline while its new identity is around says so. */
internal fun computerStatusText(state: AppState): String =
    if (state.isStale && !state.connection.unauthorized) "Older copy of this computer · ${statusText(state)}" else statusText(state)

/** A word for the link: the computer filter's rows and the chat header. */
internal fun connectionLabel(state: AppState): String = if (state.computers.isEmpty()) "Not paired" else when (val link = state.connection) {
    is LinkState.Ready -> if (state.mismatch != null) "Needs update" else "Connected"
    LinkState.Connecting -> "Connecting…"
    is LinkState.ComputerOffline -> "Offline"
    is LinkState.Failed -> if (link.unauthorized) "No access" else "Offline"
}

/** What TalkBack says for the chat header's connection: the route too, when there is one. */
internal fun connectionDescription(state: AppState): String {
    val link = state.connection
    return when {
        state.computers.isEmpty() -> "Not paired"
        link == LinkState.Connecting -> "Connecting"
        link !is LinkState.Ready -> connectionLabel(state)
        state.mismatch != null -> "Needs update"
        link.route == HostRoute.Relay -> "Connected through Cloudflare"
        else -> "Connected over Wi-Fi or Tailscale"
    }
}

/** "1 connected", "Connecting…", "No access": the roster header's summary of its computers. */
internal fun connectionSummary(state: AppState): String = when {
    state.computers.isEmpty() -> "Computers"
    state.connection is LinkState.Ready -> "${state.computers.size} connected"
    state.connection is LinkState.Connecting -> "Connecting…"
    state.connection.unauthorized -> "No access"
    else -> "${state.computers.size} offline"
}

/**
 * The computer this phone uses won't come back as it is: it turned this phone away, or it's offline
 * while an account computer with its name that joined after it was last seen is online, the same
 * machine under a new identity (iOS `AccountStore.isStale`). Account computers offer access recovery; only independently paired computers offer removal.
 */
internal val AppState.isStale: Boolean get() {
    val computer = computers.firstOrNull() ?: return false
    return when (val link = connection) {
        is LinkState.Failed -> link.unauthorized || renamedCopyOnline(computer)
        is LinkState.ComputerOffline -> renamedCopyOnline(computer)
        else -> false
    }
}

private fun AppState.renamedCopyOnline(computer: Computer): Boolean {
    val seen = cloudComputers.firstOrNull { it.computerId == computer.id }?.lastSeenAt ?: (connection as? LinkState.ComputerOffline)?.lastSeenAt
    return cloudComputers.any { it.computerId != computer.id && it.name == computer.name && it.online == true && replacedBy(it.computerId, seen) }
}

/**
 * A computer last [seen] no later than account computer [id] joined the account was replaced by it;
 * seen after, it ran beside it: another machine with the same name. Without both times the name decides.
 */
private fun AppState.replacedBy(id: String, seen: Long?): Boolean {
    seen ?: return true
    val claimed = cloudComputers.firstOrNull { it.computerId == id }?.claimedAt ?: return true
    return seen <= claimed
}

/**
 * An offline account computer named like one this phone uses or sees online, and [replacedBy] it: most
 * likely the same machine under an earlier identity (its host keys were reset), left behind in the account.
 */
internal fun AppState.isOlderCopy(computer: CloudComputer): Boolean {
    if (computer.online == true) return false
    val seen = computer.lastSeenAt
    // The current copy may be paired with its code without being listed in the account.
    return computers.any { it.id != computer.computerId && it.name == computer.name && replacedBy(it.id, seen) }
        || cloudComputers.any { other -> other.computerId != computer.computerId && other.name == computer.name
            && (computers.any { it.id == other.computerId } || other.online == true) && replacedBy(other.computerId, seen) }
}
