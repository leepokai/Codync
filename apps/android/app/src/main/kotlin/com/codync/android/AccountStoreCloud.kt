package com.codync.android

import java.io.IOException
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.sync.withLock

// Codync Cloud: the computers Codync runs for this account (Computers → Codync Cloud, docs/plans/codync-cloud.md).

/** The account's cloud computers and whether it may have them; while one is being prepared, until it joins the account. */
fun AccountStore.refreshCloudMachines() = action("cloudMachines") {
    val client = cloud ?: return@action
    try { mutableState.update { it.copy(cloudOffer = client.cloudComputers()) } }
    catch (error: IOException) {
        if (error is CloudError && !error.isTransient) throw error
        android.util.Log.i("Codync", "Cloud computers refresh failed: ${error.message}")
    }
    awaitClaim()
}

/** A Codync Cloud sheet (New cloud computer, a provider's key) opened or closed. Open, it shows the account's error itself
 * (iOS too), so the app's error dialog waits; closed, that error was seen and goes. */
internal fun AccountStore.cloudSheetShown(open: Boolean) = mutableState.update {
    it.copy(cloudSheets = (it.cloudSheets + if (open) 1 else -1).coerceAtLeast(0), error = if (open) it.error else null)
}

/** The Codync Cloud card came on screen or left it. On screen, it shows its cloud computers' errors itself, so the app's
 * error dialog leaves those to it; gone, such an error was seen and goes. */
internal fun AccountStore.cloudCardShown(shown: Boolean) {
    mutableState.update { it.copy(cloudCardShown = shown) }
    if (!shown) clearCloudError(CloudErrorPlace.Card)
}

private const val CLOUD_KEY = "cloudKey:"

/** The busy key while [provider]'s key is saved or removed. */
internal fun cloudKeyAction(provider: String) = "$CLOUD_KEY$provider"

/** The longest cloud computer name the cloud keeps. */
internal const val CLOUD_NAME_LIMIT = 64

/** The busy key while a cloud computer is created; its error shows in the New cloud computer sheet. */
internal const val CLOUD_CREATE = "cloudCreate"

/** Where a Codync Cloud action's error shows (iOS `CloudMachineStore.Place`): under its provider's key in its key sheet,
 * in New cloud computer, or on the Codync Cloud card (a cloud computer's own actions). */
internal sealed interface CloudErrorPlace {
    data class Key(val provider: String) : CloudErrorPlace
    data object Create : CloudErrorPlace
    data object Card : CloudErrorPlace

    companion object {
        /** Where the error of the [AccountStore.action] keyed [action] shows; null: not one of Codync Cloud's (the app's dialog). */
        fun of(action: String?): CloudErrorPlace? = when {
            action == null -> null
            action.startsWith(CLOUD_KEY) -> Key(action.removePrefix(CLOUD_KEY))
            action == CLOUD_CREATE -> Create
            CloudMachineAction.of(action) != null -> Card
            else -> null
        }
    }
}

/** A cloud computer's own actions, each busy on its own ([key]); their errors show on the Codync Cloud card. */
internal enum class CloudMachineAction(private val prefix: String) {
    Rename("cloudRename:"), DeleteSnapshots("cloudSnapshots:"), Delete("cloudDelete:");

    fun key(id: String) = prefix + id

    companion object {
        fun of(key: String?): CloudMachineAction? = entries.firstOrNull { key?.startsWith(it.prefix) == true }
    }
}

/** One of cloud computer [id]'s actions runs: its row shows a spinner instead of its menu. */
internal fun AccountContext.cloudMachineBusy(id: String) = CloudMachineAction.entries.any { it.key(id) in busy }

/** [AccountContext.error] where [place] shows it: only an action's that belongs there (iOS `CloudMachineStore.error(at:)`). */
internal fun AccountContext.cloudErrorAt(place: CloudErrorPlace): String? = error.takeIf { CloudErrorPlace.of(failedAction) == place }

/** A provider key's error, whichever provider's: New cloud computer shows a removal asked from a row's `⋯` there (iOS
 * `CloudMachineStore.keyRemovalError`); a save's shows in the key sheet over it, and goes when that closes. */
internal fun AccountContext.cloudKeyError(): String? = error.takeIf { CloudErrorPlace.of(failedAction) is CloudErrorPlace.Key }

/** Forgets an error shown at [place] once it's out of view. */
internal fun AccountStore.clearCloudError(place: CloudErrorPlace) = mutableState.update {
    if (CloudErrorPlace.of(it.failedAction) == place) it.copy(error = null, failedAction = null) else it
}

/** Gives the cloud the key to the user's own account with [provider]; a refusal shows the cloud's words for what to fix.
 * [saved] runs once it's kept. */
fun AccountStore.saveCloudKey(provider: String, key: String, saved: () -> Unit) = action(cloudKeyAction(provider)) {
    val credential = tracked(AnalyticsEvent.CloudKeySaved(CloudKeyOutcome.Saved, provider), { AnalyticsEvent.CloudKeySaved(CloudKeyOutcome.of(it), provider) }) {
        it.saveCloudKey(provider, key)
    }
    mutableState.update { it.copy(cloudOffer = it.cloudOffer?.withCredential(provider, credential)) }
    saved()
}

/** `409` while a cloud computer runs on [provider] shows the cloud's own words. */
fun AccountStore.removeCloudKey(provider: String) = action(cloudKeyAction(provider)) {
    tracked(AnalyticsEvent.CloudKeyRemoved(CloudRemoveOutcome.Removed, provider), { AnalyticsEvent.CloudKeyRemoved(CloudRemoveOutcome.of(it), provider) }) {
        it.deleteCloudKey(provider)
    }
    mutableState.update { it.copy(cloudOffer = it.cloudOffer?.withCredential(provider, null)) }
}

/** Another cloud computer called [name] on [provider], one whose key works; the cloud's refusal (the limit, the key)
 * shows its own words. [created] runs once it's made. */
fun AccountStore.createCloudComputer(provider: String, name: String, created: () -> Unit) = action(CLOUD_CREATE) {
    val machine = tracked(AnalyticsEvent.CloudComputerCreated(CloudCreateOutcome.Created, provider), { AnalyticsEvent.CloudComputerCreated(CloudCreateOutcome.of(it), provider) }) {
        it.createCloudComputer(provider, name)
    }
    changeMachines { it + machine }
    created()
}

/** Its new [name] (the cloud refuses an empty one); a computer this phone uses shows it before its host says so. */
fun AccountStore.renameCloudComputer(id: String, name: String) = action(CloudMachineAction.Rename.key(id)) {
    val machine = tracked(AnalyticsEvent.CloudComputerRenamed(CloudRenameOutcome.Renamed), { AnalyticsEvent.CloudComputerRenamed(CloudRenameOutcome.Failed) }) {
        it.renameCloudComputer(id, name)
    }
    changeMachines { list -> list.map { if (it.cloudComputerId == id) machine else it } }
    store(machine.computerId)?.setHostName(machine.name)
    refreshCloud()
}

/** `409` while it sleeps shows the cloud's own words. [id] is a listed computer (its row asked); one gone from the list
 * meanwhile has nothing left to act on (iOS too). */
fun AccountStore.deleteCloudSnapshots(id: String) = action(CloudMachineAction.DeleteSnapshots.key(id)) {
    val provider = state.value.cloudOffer?.machines?.firstOrNull { it.cloudComputerId == id }?.provider ?: return@action
    tracked(AnalyticsEvent.CloudSnapshotsDeleted(CloudDeleteOutcome.Deleted, provider), { AnalyticsEvent.CloudSnapshotsDeleted(CloudDeleteOutcome.of(it), provider) }) {
        it.deleteCloudSnapshots(id)
    }
}

/** Deletes it with everything on it; this phone forgets it too when it uses it (iOS `onAccountChanged`). [id] is a listed
 * computer, as for [deleteCloudSnapshots]. */
fun AccountStore.deleteCloudComputer(id: String) = action(CloudMachineAction.Delete.key(id)) {
    val machine = state.value.cloudOffer?.machines?.firstOrNull { it.cloudComputerId == id } ?: return@action
    tracked(AnalyticsEvent.CloudComputerDeleted(CloudDeleteOutcome.Deleted, machine.provider), { AnalyticsEvent.CloudComputerDeleted(CloudDeleteOutcome.of(it), machine.provider) }) {
        it.deleteCloudComputer(id)
    }
    changeMachines { list -> list.filter { it.cloudComputerId != id } }
    val computerId = machine.computerId
    if (computerId != null && store(computerId) != null) transitions.withLock { close(computerId) }
    refreshCloud()
}

/** [call] on the account's cloud, reporting what came of it: [ok] once the cloud answered, [failed] for its error. A
 * cancelled call came to nothing and reports nothing (iOS too). */
private inline fun <T> AccountStore.tracked(ok: AnalyticsEvent, failed: (Exception) -> AnalyticsEvent, call: (CloudClient) -> T): T {
    val result = try { call(requireNotNull(cloud) { "Sign in to reach your account." }) }
    catch (error: Exception) {
        if (error !is CancellationException) Analytics.capture(failed(error))
        throw error
    }
    Analytics.capture(ok)
    return result
}

/** Changes the listed cloud computers. A look in flight would bring back the list from before, so it stops; the watch for
 * claims starts over (with its timeout) on the new list. */
private suspend fun AccountStore.changeMachines(change: (List<CloudMachine>) -> List<CloudMachine>) {
    actions["cloudClaim"]?.cancelAndJoin()
    mutableState.update {
        it.copy(cloudOffer = (it.cloudOffer ?: CloudOffer(available = true)).let { offer -> offer.copy(machines = change(offer.machines)) })
    }
    awaitClaim()
}

/**
 * Polls while a new cloud computer hasn't claimed itself into the account yet; each one that has gets listed with the
 * account's computers (where [askCloudComputers] picks it up).
 */
private fun AccountStore.awaitClaim() {
    if (state.value.preparingCloud.isEmpty()) return
    action("cloudClaim") {
        val client = cloud ?: return@action
        // ponytail: fixed 5 s poll, bounded by the 30 min claim ticket; a push from the Worker if it ever matters.
        withTimeoutOrNull(30 * 60_000L) {
            while (true) {
                val preparing = state.value.preparingCloud
                if (preparing.isEmpty()) break
                delay(5_000)
                try { mutableState.update { it.copy(cloudOffer = client.cloudComputers()) } }
                catch (error: CloudError) { if (!error.isTransient) throw error }
                catch (_: IOException) { /* Asked again on the next turn. */ }
                if (state.value.cloudOffer?.machines.orEmpty().any { it.cloudComputerId in preparing && it.computerId != null }) refreshCloud()
            }
        }
    }
}

/** The cloud computers created and not yet in the account (nor failed), by id. */
internal val AccountContext.preparingCloud: Set<String>
    get() = cloudOffer?.machines.orEmpty().filter { it.preparing }.mapTo(mutableSetOf()) { it.cloudComputerId }

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
