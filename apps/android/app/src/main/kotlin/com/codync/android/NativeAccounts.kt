package com.codync.android

import android.app.Activity
import android.content.Context
import com.clerk.api.Clerk
import com.clerk.api.ClerkConfigurationOptions
import com.clerk.api.hostedauth.HostedAuthCancellationException
import com.clerk.api.network.serialization.ClerkResult
import com.clerk.api.network.serialization.errorMessage
import com.clerk.api.session.Session
import com.clerk.api.sso.OAuthProvider
import com.clerk.api.sso.SSOCancellationException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.File
import java.io.IOException

/** A sign-in that ended without an account, and why. */
internal class SignInError(val reason: SignInFailure, message: String, cause: Throwable? = null) : IOException(message, cause)

data class SavedAccount(val userId: String, val sessionId: String, val email: String, val avatar: String?)
data class AccountState(val ready: Boolean = false, val configured: Boolean = NativeAccounts.configured,
    val userId: String? = null, val accounts: List<SavedAccount> = emptyList(),
    val multiple: Boolean = false, val error: String? = null)

/** Clerk owns browser authentication, encrypted credentials and session restoration. */
object NativeAccounts {
    val configured = BuildConfig.CLERK_PUBLISHABLE_KEY.startsWith("pk_test_") || BuildConfig.CLERK_PUBLISHABLE_KEY.startsWith("pk_live_")

    fun initialize(activity: Activity) {
        if (configured) Clerk.initialize(activity, BuildConfig.CLERK_PUBLISHABLE_KEY,
            ClerkConfigurationOptions(enableDebugMode = false, telemetryEnabled = false))
    }

    fun observe(scope: CoroutineScope, update: suspend (AccountState) -> Unit) = scope.launch {
        if (!configured) { update(AccountState(ready = true)); return@launch }
        combine(Clerk.isInitialized, Clerk.sessionFlow, Clerk.sessionsFlow,
            Clerk.multiSessionModeIsEnabledFlow, Clerk.initializationError) { ready, selected, sessions, multiple, error ->
            val accounts = sessions.filter { it.status == Session.SessionStatus.ACTIVE && it.expireAt > System.currentTimeMillis() }
                .mapNotNull { session -> session.user?.let { user -> SavedAccount(user.id, session.id,
                    user.primaryEmailAddress?.emailAddress ?: "Codync account", user.imageUrl.takeIf { it.startsWith("https://") }) } }
                .distinctBy(SavedAccount::userId)
            AccountState(ready, true, selected?.usableUserId(), accounts, multiple,
                if (error != null && !ready) "Couldn't restore sign-in. Check your connection and retry." else null)
        }.collect { update(it) }
    }

    /** The signed-in account right now; `observe` reports the same once its flows catch up. */
    fun activeUserId(): String? = if (configured) Clerk.activeSession?.usableUserId() else null

    private fun Session.usableUserId(): String? = takeIf { it.status == Session.SessionStatus.ACTIVE && it.expireAt > System.currentTimeMillis() }?.user?.id

    suspend fun signIn(apple: Boolean) {
        require(Clerk.isInitialized.value) { "Wait for sign-in to finish loading." }
        val result = Clerk.auth.signInWithOAuth(if (apple) OAuthProvider.APPLE else OAuthProvider.GOOGLE)
        when (result) {
            is ClerkResult.Failure -> throw signInError(result)
            is ClerkResult.Success -> {
                val sessionId = result.value.signIn?.createdSessionId ?: result.value.signUp?.createdSessionId
                    ?: throw SignInError(SignInFailure.Verification, "Sign-in needs additional verification. Continue using the other sign-in options.")
                val activated = Clerk.auth.setActive(sessionId)
                requireSuccess(activated)
            }
        }
    }

    /**
     * A worktree's emulator signs into its sandbox's Clerk test account with a one-time token that
     * `npm run phone-account` leaves through adb's run-as, as the desktop sandbox does: debug builds against Clerk's
     * development instance only, and only while signed out.
     */
    suspend fun signInForDevelopment(context: Context) {
        if (!BuildConfig.DEBUG || !BuildConfig.CLERK_PUBLISHABLE_KEY.startsWith("pk_test_")) return
        val file = File(context.filesDir, "dev-sign-in-ticket")
        val ticket = withContext(Dispatchers.IO) { file.takeIf(File::exists)?.readText()?.trim().also { file.delete() } }
        if (ticket.isNullOrEmpty()) return
        Clerk.isInitialized.first { it }
        if (activeUserId() != null) return
        when (val result = Clerk.auth.signInWithTicket(ticket)) {
            is ClerkResult.Failure -> throw signInError(result)
            is ClerkResult.Success -> result.value.createdSessionId?.let { requireSuccess(Clerk.auth.setActive(it)) }
        }
    }

    suspend fun hostedSignIn() {
        val result = Clerk.auth.startHostedAuth()
        if (result is ClerkResult.Failure) throw signInError(result)
    }

    private fun signInError(failure: ClerkResult.Failure<com.clerk.api.network.model.error.ClerkErrorResponse>): SignInError {
        val cause = failure.throwable
        val reason = when {
            cause is SSOCancellationException || cause is HostedAuthCancellationException -> SignInFailure.Cancelled
            cause?.isTimeout == true -> SignInFailure.Timeout
            else -> SignInFailure.Error
        }
        return SignInError(reason, failure.errorMessage, cause)
    }

    suspend fun switch(account: SavedAccount) {
        val result = Clerk.auth.setActive(account.sessionId)
        requireSuccess(result)
    }

    suspend fun signOut(sessionId: String) {
        // An explicit session keeps local credentials intact if the server rejects logout.
        val result = Clerk.auth.signOut(sessionId)
        requireSuccess(result)
    }

    suspend fun reset() {
        if (!configured) return
        // The all-session API clears the SDK's local credentials even when the network is down.
        val result = Clerk.auth.signOut()
        if (result is ClerkResult.Failure) {
            val error = result.throwable
            if (error is CancellationException) throw error
        }
    }

    fun retry() { if (configured) Clerk.reinitialize() }

    suspend fun token(expectedUser: String): String {
        require(Clerk.activeSession?.user?.id == expectedUser) { "Sign in again to reach your account." }
        val result = Clerk.auth.getToken()
        require(Clerk.activeSession?.user?.id == expectedUser) { "The active account changed." }
        return when (result) {
            is ClerkResult.Success -> result.value
            is ClerkResult.Failure -> throw IOException("Couldn't refresh sign-in. Check your connection and try again.")
        }
    }

    private fun requireSuccess(result: ClerkResult<*, com.clerk.api.network.model.error.ClerkErrorResponse>) {
        if (result is ClerkResult.Failure) throw IOException(result.errorMessage)
    }
}
