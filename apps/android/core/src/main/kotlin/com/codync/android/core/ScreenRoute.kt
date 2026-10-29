package com.codync.android.core

import kotlinx.coroutines.delay

/** Presence can briefly report offline while signaling reconnects; don't spend an ICE retry yet. */
suspend fun waitForScreenRoute(current: () -> LinkState?) {
    deadline(20_000, "Couldn't reconnect to the computer.") {
        while (true) {
            when (val link = current()) {
                is LinkState.Ready -> return@deadline
                is LinkState.Failed -> if (link.status in listOf(403, 426)) {
                    throw HostError(requireNotNull(link.status), link.message)
                }
                else -> Unit
            }
            delay(100)
        }
    }
}
