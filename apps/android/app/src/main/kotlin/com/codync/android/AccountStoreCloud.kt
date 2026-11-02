package com.codync.android

import java.io.IOException
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.sync.withLock

// Codync Cloud: the computer Codync runs for this account (Computers → Codync Cloud, docs/plans/codync-cloud.md).

/** The account's cloud computer and whether it may have one; while it's being prepared, until it joins the account. */
fun AccountStore.refreshCloudMachine() = action("cloudMachine") {
    val client = cloud ?: return@action
    try { mutableState.update { it.copy(cloudOffer = client.cloudComputer()) } }
    catch (error: IOException) {
        if (error is CloudError && !error.isTransient) throw error
        android.util.Log.i("Codync", "Cloud computer refresh failed: ${error.message}")
    }
    awaitClaim()
}

/** `403` off the allowlist shows the cloud's own words. */
fun AccountStore.createCloudComputer() = action("cloudMachine") {
    val machine = requireNotNull(cloud) { "Sign in to reach your account." }.createCloudComputer()
    mutableState.update { it.copy(cloudOffer = CloudOffer(machine, available = true)) }
    awaitClaim()
}

/** `409` while it sleeps shows the cloud's own words. */
fun AccountStore.deleteCloudSnapshots() = action("cloudMachine") {
    requireNotNull(cloud) { "Sign in to reach your account." }.deleteCloudSnapshots()
}

/** Deletes it with everything on it; this phone forgets it too when it uses it (iOS `onAccountChanged`). */
fun AccountStore.deleteCloudComputer() = sessionAction {
    val client = requireNotNull(cloud) { "Sign in to reach your account." }
    val computerId = state.value.cloudOffer?.cloudComputer?.computerId
    actions["cloudClaim"]?.cancelAndJoin()
    client.deleteCloudComputer()
    mutableState.update { it.copy(cloudOffer = it.cloudOffer?.copy(cloudComputer = null)) }
    if (computerId != null && store(computerId) != null) transitions.withLock { close(computerId) }
    refreshCloud()
}

/**
 * Polls a new cloud computer until it has claimed itself into the account, then lists it with the account's
 * computers (where [askCloudComputers] picks it up).
 */
private fun AccountStore.awaitClaim() {
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
internal val AccountContext.preparingCloud: Boolean get() =
    cloudOffer?.cloudComputer?.let { it.computerId == null && !it.failed } == true

/**
 * Codync Cloud computers approve this phone by themselves: each one it can't use yet is asked once per app session,
 * through the same request and code exchange as a tap on "Ask for access" (iOS `askCloudComputers`).
 */
internal fun AccountStore.askCloudComputers(computers: List<CloudComputer>) {
    for (computer in computers) {
        if (computer.cloud == null || computer.access != "none" || store(computer.computerId) != null) continue
        if (askedCloud.add(computer.computerId)) requestAccess(computer)
    }
}

/** Every computer's store learns what the account says about it: Codync Cloud runs it, and the cloud's floor for this app. */
internal fun AccountStore.shareAccountView() {
    val context = state.value
    for (store in stores.value) store.setAccountView(context.cloudOf(store.id), context.appFloor)
}
