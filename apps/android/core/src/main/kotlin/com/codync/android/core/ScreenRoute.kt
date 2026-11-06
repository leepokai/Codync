package com.codync.android.core

import kotlinx.coroutines.delay

/** Statuses that end screen recovery: access was taken away, or the app must update. */
val screenTerminalStatuses = listOf(403, 426)

/**
 * Waits out a route outage (presence can briefly report offline while signaling reconnects) so it doesn't spend an
 * ICE retry; like the iPhone, only a denial or a required update ends it. Closing the screen cancels the wait.
 */
suspend fun waitForScreenRoute(current: () -> LinkState?) {
    while (true) {
        when (val link = current()) {
            is LinkState.Ready -> return
            is LinkState.Failed -> if (link.status in screenTerminalStatuses) {
                throw HostError(requireNotNull(link.status), link.message)
            }
            else -> Unit
        }
        delay(100)
    }
}

/**
 * Whether attempt number [failedAttempts] asks for TURN, the screen's way through Cloudflare, by the computer's
 * Connection choice; the other way gets the next attempt when one fails (iOS `ScreenRoute`). [channelOnRelay]: chat
 * itself reaches the computer through a relay. [hasTurn]: the computer is in an account on the Codync cloud; a computer
 * in no account and a private relay have none.
 */
fun screenAsksForTurn(channelOnRelay: Boolean, route: ConnectionRoute, hasTurn: Boolean, failedAttempts: Int): Boolean {
    val later = failedAttempts % 2 == 1
    return when (route) {
        ConnectionRoute.DirectOnly -> false
        ConnectionRoute.CloudflareFirst -> if (hasTurn) !later else channelOnRelay
        ConnectionRoute.Automatic -> channelOnRelay || (hasTurn && later)
    }
}
