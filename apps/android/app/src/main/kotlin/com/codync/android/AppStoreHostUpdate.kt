package com.codync.android

import com.codync.android.core.AppVersion
import com.codync.android.core.LinkState
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

/**
 * Out of reach while an update asked from here installs (iOS `BotStore.isUpdating`): the computer restarts on its own,
 * so it reads as updating, not as a lost connection or access to ask for.
 */
internal val AppState.isUpdating get() = connection !is LinkState.Ready && hostUpdate.updating(System.currentTimeMillis())

/** What the notice says about a computer that's behind, and whether it offers Update now. */
internal data class HostUpdateNotice(val detail: String, val offersUpdate: Boolean)

internal fun hostUpdateNotice(host: String, mismatch: VersionMismatch.UpdateHost, os: String?, canUpdate: Boolean,
    update: HostUpdate?, now: Long): HostUpdateNotice {
    if (update.updating(now)) return HostUpdateNotice(updatingText(host), false)
    val needs = "$host runs Codync ${mismatch.version}; this app needs ${mismatch.minimum} or newer."
    if (!canUpdate) return HostUpdateNotice("$needs ${howToUpdate(os)}", false)
    return HostUpdateNotice(when (update) {
        is HostUpdate.Failed -> "$needs ${update.message}"
        is HostUpdate.Requested -> "$needs ${notUpdatedText(host)}"
        null -> needs
    }, true)
}

private fun updatingText(host: String) = "Updating Codync on $host. This takes a few minutes."
private fun notUpdatedText(host: String) = "$host didn't finish updating. Try again, or update Codync on that computer."

/** How to update Codync on a computer this app can't update (iOS `UpdateNeededText.how`). */
private fun howToUpdate(os: String?) = when (os) {
    "macos" -> "Open Codync on that Mac and choose Check for Updates."
    "linux" -> "Run codync-host update there."
    else -> "Update Codync on that computer."
}

/**
 * The Codync row on the computer's card in Computer management: the version it runs, and an update when
 * one is due, a newer release or a version too old for this app (iOS `HostVersionLine`). `detail` is
 * sentences, each its own phrase to translate.
 */
internal data class HostVersionLine(val version: String, val detail: List<String>, val warns: Boolean = false,
    val accessory: Accessory? = null) {
    /** What the row ends with: a spinner while the computer updates, or the button that asks it to. */
    sealed interface Accessory {
        data object Updating : Accessory
        data class Button(val title: String) : Accessory
    }
}

/** `latest` is the latest release the computer found (null while it doesn't know: then no verdict). */
internal fun hostVersionLine(version: String, latest: String?, mismatch: VersionMismatch?, canUpdate: Boolean,
    update: HostUpdate?, host: String, os: String?, now: Long): HostVersionLine {
    if (update.updating(now)) return HostVersionLine(version, listOf(updatingText(host)), accessory = HostVersionLine.Accessory.Updating)
    val minimum = (mismatch as? VersionMismatch.UpdateHost)?.minimum
    val available = latest?.takeIf { AppVersion.isBelow(version, it) }
    val due = minimum?.let { "This app needs $it or newer." } ?: available?.let { "Version $it is available." }
        ?: return HostVersionLine(version, if (latest == null) emptyList() else listOf("Up to date"))
    if (!canUpdate) return HostVersionLine(version, listOf(due, howToUpdate(os)), warns = minimum != null)
    val tryAgain = HostVersionLine.Accessory.Button("Try again")
    return when (update) {
        is HostUpdate.Failed -> HostVersionLine(version, listOf(update.message), warns = true, accessory = tryAgain)
        is HostUpdate.Requested -> HostVersionLine(version, listOf(notUpdatedText(host)), warns = true, accessory = tryAgain)
        null -> HostVersionLine(version, listOf(due), warns = minimum != null, accessory = HostVersionLine.Accessory.Button("Update"))
    }
}

/** This computer's Codync row at `now`; null until its `hello` arrives. */
internal fun AppState.hostVersionLine(now: Long): HostVersionLine? {
    val hello = hello ?: return null
    val version = hello.text("version") ?: return null
    return hostVersionLine(version, hello.text("latestVersion"), mismatch, canUpdateHost, hostUpdate, hostName, hello.text("os"), now)
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
