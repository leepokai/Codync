package com.codync.android

import android.content.Context
import android.hardware.biometrics.BiometricManager
import android.hardware.biometrics.BiometricPrompt
import android.os.Build
import android.os.CancellationSignal
import android.os.SystemClock
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalContext
import com.codync.android.core.SensitiveAuthorization
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine

/**
 * An optional lock on actions that reveal or replace secrets (webhook keys, saved credentials), using the phone's
 * biometrics or screen lock. It only guards this device: the computer still decides what any request may do, and a
 * successful unlock never travels to it. The unlock lasts a minute, ends when the app leaves the foreground and
 * never carries to another account.
 */
internal object SensitiveAccess {
    private const val PREFS = "codync-security"
    private const val REQUIRED = "requireUnlock"
    private val authorization = SensitiveAuthorization()
    private val authenticators: Int get() =
        if (Build.VERSION.SDK_INT >= 30) BiometricManager.Authenticators.BIOMETRIC_STRONG or BiometricManager.Authenticators.DEVICE_CREDENTIAL else 0

    fun required(context: Context): Boolean = prefs(context).getBoolean(REQUIRED, false)
    fun require(context: Context, on: Boolean) { prefs(context).edit().putBoolean(REQUIRED, on).apply(); if (!on) revoke() }
    fun revoke() = authorization.revoke()

    /** Whether this phone has a biometric or a screen lock to ask for. */
    fun available(context: Context): Boolean = biometrics(context) || secureLock(context)

    private fun biometrics(context: Context): Boolean {
        val manager = context.getSystemService(BiometricManager::class.java) ?: return false
        @Suppress("DEPRECATION") // Android 10 has no per-authenticator query; the default is strong biometrics.
        return if (Build.VERSION.SDK_INT >= 30) manager.canAuthenticate(authenticators) == BiometricManager.BIOMETRIC_SUCCESS
            else manager.canAuthenticate() == BiometricManager.BIOMETRIC_SUCCESS
    }

    private fun secureLock(context: Context): Boolean =
        context.getSystemService(android.app.KeyguardManager::class.java)?.isDeviceSecure == true

    /** True once unlocked (or when no unlock is required); false on cancel, failure or no screen lock to ask for. */
    suspend fun confirm(context: Context, contextId: String, reason: String): Boolean {
        if (!required(context)) return true
        val now = SystemClock.elapsedRealtime()
        if (authorization.isValid(contextId, now)) return true
        if (!available(context)) return false
        val unlocked = prompt(context, reason)
        if (unlocked) authorization.grant(contextId, SystemClock.elapsedRealtime())
        return unlocked
    }

    /** Asks for the phone unlock whatever the secrets switch says (the app lock has its own switch). */
    suspend fun ask(context: Context, reason: String): Boolean =
        if (available(context)) prompt(context, reason) else false

    private suspend fun prompt(context: Context, reason: String): Boolean = suspendCancellableCoroutine { continuation ->
        val cancel = CancellationSignal()
        continuation.invokeOnCancellation { cancel.cancel() }
        val builder = BiometricPrompt.Builder(context).setTitle("Unlock Codync").setSubtitle(reason)
        @Suppress("DEPRECATION")
        // Android 10 can't combine both: ask for the enrolled biometrics, and fall back to the screen lock only
        // when the phone has none. From Android 11 the platform offers both in one prompt.
        if (Build.VERSION.SDK_INT >= 30) builder.setAllowedAuthenticators(authenticators)
        else if (!biometrics(context)) builder.setDeviceCredentialAllowed(true)
        builder.build().authenticate(cancel, context.mainExecutor, object : BiometricPrompt.AuthenticationCallback() {
            override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult?) { if (continuation.isActive) continuation.resumeWith(Result.success(true)) }
            override fun onAuthenticationError(errorCode: Int, errString: CharSequence?) { if (continuation.isActive) continuation.resumeWith(Result.success(false)) }
        })
    }

    private fun prefs(context: Context) = context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
}

/**
 * Runs a sensitive action after the unlock, if the user turned it on; a cancelled unlock never runs it and says why.
 * `refused` gets the message to show.
 */
@Composable internal fun rememberUnlock(contextId: String, scope: CoroutineScope, refused: (String) -> Unit): (String, () -> Unit) -> Unit {
    val context = LocalContext.current
    return remember(context, contextId, scope) {
        { reason, action ->
            scope.launch {
                if (SensitiveAccess.confirm(context, contextId, reason)) action()
                else refused("Unlock this phone to continue.")
            }
        }
    }
}
