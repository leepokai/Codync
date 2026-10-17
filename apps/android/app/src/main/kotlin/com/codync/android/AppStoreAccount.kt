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

// Accounts, account computers and starting over: what the account sheet and Computers & settings ask for.

fun AppStore.signIn(apple: Boolean = false, hosted: Boolean = false) = sessionAction {
    if (hosted) NativeAccounts.hostedSignIn() else NativeAccounts.signIn(apple)
}
fun AppStore.switchAccount(account: SavedAccount) = sessionAction { NativeAccounts.switch(account) }
fun AppStore.retryAccounts() { NativeAccounts.retry() }
fun AppStore.signOut() = sessionAction {
    val old = state.value.account.accounts.firstOrNull { it.userId == state.value.account.userId } ?: return@sessionAction
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
        val target = computers.firstOrNull { remote -> state.value.computers.isEmpty() && remote.computerId !in asked }
        if (target != null) requestAccess(target)
    } catch (error: CloudError) { if (!error.isTransient) throw error }
}

fun AppStore.requestAccess(target: CloudComputer) = action("access") {
    val client = requireNotNull(cloud) { "Sign in to reach your account." }
    require(state.value.computers.isEmpty() || state.value.computers.first().id == target.computerId) { "Forget the current computer before connecting a different one." }
    asked.add(target.computerId)
    val ticket = client.requestAccess(target.computerId, target.signKey)
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
                    val computer = Computer(ticket.computerId, target.name, ticket.signKey, cloud = BuildConfig.CLOUD_URL)
                    computer.validate()
                    withContext(Dispatchers.IO) { local.saveComputers(listOf(computer)); local.completeSetup() }
                    mutableState.update { it.copy(computers = listOf(computer), setupComplete = true) }
                    openMirror(); connect(); return@action
                }
                "denied" -> throw IOException("${target.name} declined the request.")
                else -> throw CloudError(410, "requestExpired")
            }
        }
    } finally { mutableState.update { it.copy(access = null) } }
}

fun AppStore.cancelAccess() {
    val ticket = state.value.access
    val client = cloud
    val request = actions["access"]
    action("cancelAccess") {
        request?.cancelAndJoin()
        if (ticket != null && client != null) client.cancelAccess(ticket.requestId)
    }
}

fun AppStore.forgetComputer() = sessionAction {
    transitions.withLock {
        val userId = state.value.account.userId
        val id = state.value.computers.firstOrNull()?.id
        retireContext()
        if (id != null) asked.add(id)
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
            mutableState.update { it.copy(account = AccountState(ready = true), error = null) }
            loadContext(null)
        }
    }
}
