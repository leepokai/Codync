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
