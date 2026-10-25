package com.codync.android

import com.codync.android.core.VersionMismatch
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.update
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonPrimitive

// Updating a computer that's behind this app, from here (iOS `BotStore+HostUpdate`).

/** An update this phone asked of the computer: waiting for it to come back newer, or why it couldn't start. */
sealed interface HostUpdate {
    data class Requested(val at: Long) : HostUpdate
    data class Failed(val message: String) : HostUpdate
}

/** How long a requested update shows as under way before the card offers it again. */
internal const val HOST_UPDATE_WAIT_MS = 10 * 60_000L

/** `installHostUpdate` can update this computer from here (missing from older hosts: it can't). */
internal val AppState.canUpdateHost get() = hello?.get("canUpdate")?.jsonPrimitive?.booleanOrNull == true

/** A requested update still within its wait: the computer is updating. */
internal fun HostUpdate?.updating(now: Long) = this is HostUpdate.Requested && now - at < HOST_UPDATE_WAIT_MS

/** What the notice says about a computer that's behind, and whether it offers Update now. */
internal data class HostUpdateNotice(val detail: String, val offersUpdate: Boolean)

internal fun hostUpdateNotice(host: String, mismatch: VersionMismatch.UpdateHost, os: String?, canUpdate: Boolean,
    update: HostUpdate?, now: Long): HostUpdateNotice {
    if (update.updating(now)) return HostUpdateNotice("Updating Codync on $host. This takes a few minutes.", false)
    val needs = "$host runs Codync ${mismatch.version}; this app needs ${mismatch.minimum} or newer."
    if (!canUpdate) return HostUpdateNotice("$needs " + when (os) {
        "macos" -> "Open Codync on that Mac and choose Check for Updates."
        "linux" -> "Run codync-host update there."
        else -> "Update Codync on that computer."
    }, false)
    return HostUpdateNotice(when (update) {
        is HostUpdate.Failed -> "$needs ${update.message}"
        is HostUpdate.Requested -> "$needs $host didn't finish updating. Try again, or update Codync on that computer."
        null -> needs
    }, true)
}

/** Asks a computer that's behind to install the latest release (the update card's Update now). */
fun AppStore.requestHostUpdate() = action("hostUpdate") {
    mutableState.update { it.copy(hostUpdate = HostUpdate.Requested(System.currentTimeMillis())) }
    try { ready().call("installHostUpdate") }
    catch (error: CancellationException) { throw error }
    catch (error: Exception) {
        mutableState.update { it.copy(hostUpdate = HostUpdate.Failed(error.message ?: "Couldn't start the update.")) }
    }
}
