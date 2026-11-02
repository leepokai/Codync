package com.codync.android

import android.app.Application
import android.os.Build
import java.io.File
import java.io.IOException
import kotlinx.coroutines.sync.withLock
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*

// Accounts, account computers and starting over: what the account sheet, Computer management and Settings ask for.

internal fun AccountStore.signIn(provider: SignInProvider) = sessionAction {
    Analytics.capture(AnalyticsEvent.SignInStarted(provider))
    try {
        if (provider == SignInProvider.Hosted) NativeAccounts.hostedSignIn() else NativeAccounts.signIn(apple = provider == SignInProvider.Apple)
    } catch (error: CancellationException) { throw error }
    catch (error: Exception) {
        Analytics.capture(AnalyticsEvent.SignInFailed(provider, (error as? SignInError)?.reason ?: SignInFailure.Error))
        throw error
    }
    // Clerk holds the new session already, so this counts toward the account.
    Analytics.capture(AnalyticsEvent.SignedIn(provider))
}
fun AccountStore.switchAccount(account: SavedAccount) = sessionAction { NativeAccounts.switch(account) }
fun AccountStore.retryAccounts() { NativeAccounts.retry() }
fun AccountStore.signOut() = sessionAction {
    val old = state.value.account.accounts.firstOrNull { it.userId == state.value.account.userId } ?: return@sessionAction
    // Sent first so it still counts toward the account.
    Analytics.capture(AnalyticsEvent.SignedOut)
    NativeAccounts.signOut(old.sessionId)
    transitions.withLock {
        // The SDK updates its session synchronously; its observer may be queued behind us.
        val userId = com.clerk.api.Clerk.activeSession?.user?.id
        retireContext()
        loadContext(userId)
        val remaining = com.clerk.api.Clerk.auth.sessions.any { it.user?.id == old.userId && it.status == com.clerk.api.session.Session.SessionStatus.ACTIVE }
        if (!remaining) withContext(Dispatchers.IO) {
            LocalStore(getApplication(), old.userId).erase()
            File(getApplication<Application>().cacheDir, "codync-images-${LocalStore.referenceId(old.userId)}").deleteRecursively()
        }
    }
}

fun AccountStore.refreshCloud() = action("cloud") {
    val client = cloud ?: return@action
    try {
        client.registerDevice(Build.MODEL.take(100))
        val computers = client.computers()
        mutableState.update { it.copy(cloudComputers = computers) }
        // Signed in to an account with computers, before pairing any (iOS: the same moment).
        if (computers.isNotEmpty() && state.value.computers.isEmpty()) Analytics.completeOnboarding(OnboardingVia.Account)
    } catch (error: IOException) {
        if (error is CloudError && !error.isTransient) throw error
        // Runs on every launch and foreground: a dropped connection, a busy cloud or a token not ready
        // yet fixes itself on the next refresh, so only answers that stay reach the dialog (as on iOS).
        android.util.Log.i("Codync", "Cloud refresh failed: ${error.message}")
    }
}

/** A pull to refresh: the account's computers, joining a refresh already under way. */
suspend fun AccountStore.refreshComputers() { refreshCloud(); actions["cloud"]?.join() }

/** The roster's pull to refresh: every computer reconnects, and the account's computers are asked for again (iOS `refreshable`). */
suspend fun AccountStore.refreshRoster() {
    for (store in stores.value) store.reconnect()
    refreshComputers()
}

/**
 * Asks [target] for access; the code arrives in `access`, where it must match the one the computer shows.
 * Asking again while it runs joins it; [renew] revokes this phone's earlier grant and asks anew (iOS `requestAccess`).
 */
fun AccountStore.requestAccess(target: CloudComputer, renew: Boolean = false) {
    val key = "access:" + target.computerId
    if (renew) {
        action("renewAccess:" + target.computerId) {
            // Await cancellation cleanup before replacing this phone's grant and request.
            actions[key]?.cancelAndJoin()
            startAccess(target, renew = true)
        }
    } else startAccess(target, renew = false)
}

private fun AccountStore.startAccess(target: CloudComputer, renew: Boolean) = action("access:" + target.computerId) {
    mutableState.update { it.copy(asking = it.asking + target.computerId) }
    try { askForAccess(target, renew) } finally { mutableState.update { it.copy(asking = it.asking - target.computerId) } }
}

private suspend fun AccountStore.askForAccess(target: CloudComputer, renew: Boolean) {
    val client = requireNotNull(cloud) { "Sign in to reach your account." }
    if (renew) client.revokeAccess(target.computerId)
    val ticket = client.requestAccess(target.computerId, target.signKey)
    Analytics.capture(AnalyticsEvent.AccessRequested)
    mutableState.update { it.copy(access = it.access + (ticket.computerId to ticket)) }
    try {
        while (currentCoroutineContext().isActive) {
            if (System.currentTimeMillis() >= ticket.expiresAt) throw CloudError(410, "requestExpired")
            delay(2_000)
            val status = try { client.accessStatus(ticket.requestId) }
            catch (error: CloudError) { if (error.isTransient) continue else throw error }
            require(status.requestId == ticket.requestId && (status.computerId == null || status.computerId == ticket.computerId)) { "Invalid access response" }
            when (status.state) {
                "pending" -> continue
                "approved" -> {
                    // Pin only the key used in the SAS. The first authenticated hello supplies addresses and box key.
                    val previous = state.value.computers.firstOrNull { it.id == ticket.computerId && it.signKey == ticket.signKey }
                    val computer = Computer(ticket.computerId, target.name, ticket.signKey, cloud = BuildConfig.CLOUD_URL)
                        .copy(urls = previous?.urls.orEmpty(), boxKey = previous?.boxKey)
                        .keepingDevicePicks(previous)
                    computer.validate()
                    Analytics.capture(AnalyticsEvent.ComputerPaired(PairedVia.Account, computer.id))
                    useApprovedComputer(computer, ticket.requestId)
                    return
                }
                "denied" -> {
                    Analytics.capture(AnalyticsEvent.AccessRequestFailed(AccessFailure.Denied))
                    throw IOException("${target.name} declined the request.")
                }
                "cancelled" -> {
                    Analytics.capture(AnalyticsEvent.AccessRequestFailed(AccessFailure.Cancelled))
                    // Withdrawn by the account: the list shows what's left of it.
                    refreshCloud()
                    throw IOException("The request was withdrawn: ${target.name} or this phone was removed from your account.")
                }
                else -> throw CloudError(410, "requestExpired")
            }
        }
    } catch (error: CloudError) {
        if (error.code == "requestExpired") Analytics.capture(AnalyticsEvent.AccessRequestFailed(AccessFailure.Expired))
        throw error
    } finally { mutableState.update { it.copy(access = it.access - ticket.computerId) } }
}

/** The approved computer joins this phone's computers (or replaces its older pairing), then connects. */
private suspend fun AccountStore.useApprovedComputer(computer: Computer, requestId: String) {
    // Approval is already confirmed by the cloud.
    mutableState.update { it.copy(approvedAccess = it.approvedAccess + (computer.id to requestId)) }
    transitions.withLock { addComputer(computer) }
    withContext(Dispatchers.IO) { local.completeSetup() }
    mutableState.update { it.copy(setupComplete = true) }
    refreshCloud()
}

/** Saves [computer] (a new one goes last, a known one keeps its place) and opens its store afresh. */
internal suspend fun AccountStore.addComputer(computer: Computer) {
    store(computer.id)?.retire()
    saveComputer(computer)
    open(computer)
}

fun AccountStore.cancelAccess(computerId: String) {
    val ticket = state.value.access[computerId]
    val client = cloud
    val request = actions["access:$computerId"]
    if (ticket != null) Analytics.capture(AnalyticsEvent.AccessRequestFailed(AccessFailure.Cancelled))
    action("cancelAccess:$computerId") {
        request?.cancelAndJoin()
        if (ticket != null && client != null) client.cancelAccess(ticket.requestId)
    }
}

/**
 * Takes back this phone's account access to a computer (its grants) and forgets the computer here.
 * Also the way out when an earlier install was approved but this one never checked the code.
 */
fun AccountStore.revokeAccess(computerId: String) = action("revoke:$computerId") {
    requireNotNull(cloud) { "Sign in to reach your account." }.revokeAccess(computerId)
    if (store(computerId) != null) transitions.withLock { close(computerId) }
    refreshCloud()
}

/**
 * Remove: forgets the computer here. Approved through the account, its grant goes too (as on iOS), or it
 * would come straight back as an account row; a failed revoke still forgets it, and its error shows.
 */
fun AccountStore.removeComputer(computerId: String) = sessionAction {
    val granted = state.value.cloudComputers.any { it.computerId == computerId && it.isGranted }
    val failure = if (!granted) null else try { requireNotNull(cloud) { "Sign in to reach your account." }.revokeAccess(computerId); null }
        catch (error: CancellationException) { throw error }
        catch (error: Exception) { error }
    transitions.withLock { close(computerId) }
    if (granted) refreshCloud()
    failure?.let { throw it }
}

fun AccountStore.skipSetup() = action("setup") {
    withContext(Dispatchers.IO) { local.completeSetup() }
    mutableState.update { it.copy(setupComplete = true) }
    Analytics.completeOnboarding(OnboardingVia.Skipped)
}

fun AccountStore.taskNotifications(enabled: Boolean) = action("taskSettings") {
    PushPreferences.tasks(getApplication(), enabled)
    mutableState.update { it.copy(tasks = enabled) }
    Analytics.capture(AnalyticsEvent.SettingChanged(PhoneSetting.LiveActivities, enabled))
    if (!enabled) NotificationDisplay.cancel(getApplication(), 2)
    for (store in stores.value) store.syncPush()
    PushDelivery.schedule(getApplication())
}

fun AccountStore.startOver() = sessionAction {
    transitions.withLock {
        retireContext()
        withContext(NonCancellable) {
            NativeAccounts.reset()
            withContext(Dispatchers.IO) {
                LocalStore.eraseAll(getApplication())
                WidgetStore.reset(getApplication())
                PushPreferences.reset(getApplication())
                val cache = getApplication<Application>().cacheDir
                File(cache, "shared-attachments").deleteRecursively()
                cache.listFiles()?.filter { it.name.startsWith("codync-images") }?.forEach { it.deleteRecursively() }
            }
            Analytics.forgetOnboarding()
            mutableState.update { it.copy(account = AccountState(ready = true), error = null) }
            loadContext(null)
        }
    }
}
