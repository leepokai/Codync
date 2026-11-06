package com.codync.android

import android.app.Application
import android.os.Build
import java.io.File
import java.io.IOException
import kotlinx.coroutines.sync.withLock
import com.codync.android.core.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.json.*

// Accounts, account computers and starting over: what the account sheet, Computer management and Settings ask for.

internal fun AppStore.signIn(provider: SignInProvider) = sessionAction {
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
fun AppStore.switchAccount(account: SavedAccount) = sessionAction { NativeAccounts.switch(account) }
fun AppStore.retryAccounts() { NativeAccounts.retry() }
fun AppStore.signOut() = sessionAction {
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

fun AppStore.refreshCloud() = action("cloud") {
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
suspend fun AppStore.refreshComputers() { refreshCloud(); actions["cloud"]?.join() }

fun AppStore.requestAccess(target: CloudComputer, renew: Boolean = false) {
    if (renew) {
        action("renewAccess") {
            // Await cancellation cleanup before replacing this phone's grant and request.
            actions["access"]?.cancelAndJoin()
            startAccess(target, renew = true)
        }
    } else startAccess(target, renew = false)
}

private fun AppStore.startAccess(target: CloudComputer, renew: Boolean) = action("access") {
    mutableState.update { it.copy(askingAccess = target.computerId) }
    try { askForAccess(target, renew) } finally { mutableState.update { it.copy(askingAccess = null) } }
}

private suspend fun AppStore.askForAccess(target: CloudComputer, renew: Boolean) {
    val client = requireNotNull(cloud) { "Sign in to reach your account." }
    if (renew) client.revokeAccess(target.computerId)
    val ticket = client.requestAccess(target.computerId, target.signKey)
    Analytics.capture(AnalyticsEvent.AccessRequested)
    mutableState.update { it.copy(access = ticket) }
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
                        .copy(urls = previous?.urls.orEmpty(), boxKey = previous?.boxKey,
                            color = previous?.color, route = previous?.route ?: ConnectionRoute.Automatic)
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
    } finally { mutableState.update { it.copy(access = null) } }
}

/** Switch only after approval, retiring the old channel and its writers before replacing its mirror. */
internal fun AppStore.useApprovedComputer(approved: Computer, requestId: String) {
    val computer = approved.keepingDevicePicks(state.value.computers.firstOrNull { it.id == approved.id && it.signKey == approved.signKey })
    // Approval is already confirmed by the cloud, even when switching must wait.
    mutableState.update { it.copy(approvedAccess = it.approvedAccess + (computer.id to requestId)) }
    val contextId = state.value.contextId
    val generation = state.value.generation
    sessionAction(wait = true) {
        transitions.withLock {
            if (state.value.contextId != contextId || state.value.generation != generation) return@withLock
            val userId = state.value.account.userId
            retireContext()
            try {
                withContext(Dispatchers.IO) { local.saveComputers(listOf(computer)); local.completeSetup() }
            } finally {
                loadContext(userId)
            }
            mutableState.update { it.copy(approvedAccess = it.approvedAccess + (computer.id to requestId)) }
        }
    }
}

fun AppStore.cancelAccess() {
    val ticket = state.value.access
    val client = cloud
    val request = actions["access"]
    if (ticket != null) Analytics.capture(AnalyticsEvent.AccessRequestFailed(AccessFailure.Cancelled))
    action("cancelAccess") {
        request?.cancelAndJoin()
        if (ticket != null && client != null) client.cancelAccess(ticket.requestId)
    }
}

/**
 * Takes back this phone's account access to a computer (its grants) and forgets the computer here.
 * Also the way out when an earlier install was approved but this one never checked the code.
 */
fun AppStore.revokeAccess(computerId: String) = action("revoke") {
    requireNotNull(cloud) { "Sign in to reach your account." }.revokeAccess(computerId)
    if (state.value.computers.firstOrNull()?.id == computerId) forgetComputer() else refreshCloud()
}

/** The computer's badge color, picked on this phone (an `AvatarPalette` id). */
fun AppStore.setColor(color: String) = action("color") { changeComputer { it.copy(color = color) } }

/**
 * Remove: forgets the computer here. Approved through the account, its grant goes too (as on iOS), or it
 * would come straight back as an account row; a failed revoke still forgets it, and its error shows.
 */
fun AppStore.removeComputer() = sessionAction {
    val id = state.value.computers.firstOrNull()?.id ?: return@sessionAction
    val granted = state.value.cloudComputers.any { it.computerId == id && it.isGranted }
    val failure = if (!granted) null else try { requireNotNull(cloud) { "Sign in to reach your account." }.revokeAccess(id); null }
        catch (error: CancellationException) { throw error }
        catch (error: Exception) { error }
    forget()
    // Thrown after the context reloads, which clears the error shown.
    failure?.let { throw it }
}

fun AppStore.forgetComputer() = sessionAction { forget() }

private suspend fun AppStore.forget() {
    transitions.withLock {
        val userId = state.value.account.userId
        retireContext()
        withContext(Dispatchers.IO) {
            local.saveComputers(emptyList())
            local.directory.listFiles()?.filter { it.name.startsWith("mirror-") || it.name.startsWith("drafts-") }?.forEach { it.delete() }
        }
        loadContext(userId)
    }
}

fun AppStore.skipSetup() = action("setup") {
    withContext(Dispatchers.IO) { local.completeSetup() }
    mutableState.update { it.copy(setupComplete = true) }
    Analytics.completeOnboarding(OnboardingVia.Skipped)
}

fun AppStore.clearError() { mutableState.update { it.copy(error = null) } }

fun AppStore.startOver() = sessionAction {
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
