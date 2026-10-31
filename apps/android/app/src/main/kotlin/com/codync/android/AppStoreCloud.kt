package com.codync.android

import java.io.IOException
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*

// Codync Cloud: the computer Codync runs for this account (Settings → Codync Cloud, docs/plans/codync-cloud.md).

/** The account's cloud computer and whether it may have one; while it's being prepared, until it joins the account. */
fun AppStore.refreshCloudMachine() = action("cloudMachine") {
    val client = cloud ?: return@action
    try { mutableState.update { it.copy(cloudOffer = client.cloudComputer()) } }
    catch (error: IOException) {
        if (error is CloudError && !error.isTransient) throw error
        android.util.Log.i("Codync", "Cloud computer refresh failed: ${error.message}")
    }
    awaitClaim()
}

/** `403` off the allowlist shows the cloud's own words. */
fun AppStore.createCloudComputer() = action("cloudMachine") {
    val machine = requireNotNull(cloud) { "Sign in to reach your account." }.createCloudComputer()
    mutableState.update { it.copy(cloudOffer = CloudOffer(machine, available = true)) }
    awaitClaim()
}

/** `409` while it sleeps shows the cloud's own words. */
fun AppStore.deleteCloudSnapshots() = action("cloudMachine") {
    requireNotNull(cloud) { "Sign in to reach your account." }.deleteCloudSnapshots()
}

/** Deletes it with everything on it; this phone forgets it too when it's the computer in use. */
fun AppStore.deleteCloudComputer() = sessionAction {
    val client = requireNotNull(cloud) { "Sign in to reach your account." }
    val computerId = state.value.cloudOffer?.cloudComputer?.computerId
    actions["cloudClaim"]?.cancelAndJoin()
    client.deleteCloudComputer()
    mutableState.update { it.copy(cloudOffer = it.cloudOffer?.copy(cloudComputer = null)) }
    if (computerId != null && state.value.computers.firstOrNull()?.id == computerId) forget() else refreshCloud()
}

/**
 * Polls a new cloud computer until it has claimed itself into the account, then lists it with the account's
 * computers (where [askCloudComputer] picks it up).
 */
private fun AppStore.awaitClaim() {
    if (!state.value.preparingCloud) return
    action("cloudClaim") {
        val client = cloud ?: return@action
        // ponytail: fixed 5 s poll, bounded by the 30 min claim ticket; a push from the Worker if it ever matters.
        withTimeoutOrNull(30 * 60_000L) {
            while (state.value.preparingCloud) {
                delay(5_000)
                try { mutableState.update { it.copy(cloudOffer = client.cloudComputer()) } }
                catch (error: CloudError) { if (!error.isTransient) throw error }
                catch (_: IOException) { /* Asked again on the next turn. */ }
            }
        }
        refreshCloud()
    }
}

/** Created, and not yet in the account (nor failed). */
internal val AppState.preparingCloud: Boolean get() =
    cloudOffer?.cloudComputer?.let { it.computerId == null && !it.failed } == true

/**
 * A cloud computer approves this phone by itself, so the app asks for access instead of waiting for the tap, once
 * per app session per computer. This phone uses one computer at a time: approval would replace the one paired, so
 * it asks only while none is.
 */
internal fun AppStore.askCloudComputer(computers: List<CloudComputer>) {
    if (state.value.computers.isNotEmpty() || state.value.askingAccess != null) return
    val target = computers.firstOrNull { it.cloud != null && it.access == "none" && it.computerId !in askedCloud } ?: return
    askedCloud += target.computerId
    requestAccess(target)
}
