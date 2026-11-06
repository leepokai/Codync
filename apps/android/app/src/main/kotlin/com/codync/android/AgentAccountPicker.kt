package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.codync.android.core.AgentAccount
import com.codync.android.design.ChoicePicker
import com.codync.android.design.CodyncDialog
import com.codync.android.design.DialogAction
import com.codync.android.design.IconButton
import com.codync.android.design.Palette
import com.codync.android.design.SecondaryButton
import com.codync.android.design.Spinner
import com.codync.android.design.Type
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch
import kotlin.coroutines.coroutineContext

/** The host owns accounts and sign-in; this control owns only its loading and selection UI. */
@Composable internal fun AgentAccountPicker(
    backend: String,
    selection: String?,
    online: Boolean,
    contextKey: String,
    supported: Boolean,
    canSignOut: Boolean,
    load: suspend () -> List<AgentAccount>,
    authenticate: suspend () -> AgentAccount,
    remove: suspend (String) -> Unit,
    signOut: suspend () -> Unit,
    choose: (String?) -> Unit,
) {
    if (!supported) return
    key(contextKey, backend, online) {
        AccountPicker(backend, selection, online, canSignOut, load, authenticate, remove, signOut, choose)
    }
}

@Composable private fun AccountPicker(
    backend: String,
    selection: String?,
    online: Boolean,
    canSignOut: Boolean,
    load: suspend () -> List<AgentAccount>,
    authenticate: suspend () -> AgentAccount,
    remove: suspend (String) -> Unit,
    signOut: suspend () -> Unit,
    choose: (String?) -> Unit,
) {
    var accounts by remember { mutableStateOf(emptyList<AgentAccount>()) }
    var loading by remember { mutableStateOf(online) }
    var signingIn by remember { mutableStateOf(false) }
    var removing by remember { mutableStateOf<String?>(null) }
    var signingOut by remember { mutableStateOf(false) }
    var failure by remember { mutableStateOf<String?>(null) }
    var retry by remember { mutableIntStateOf(0) }
    var confirmRemove by remember { mutableStateOf<AgentAccount?>(null) }
    var confirmSignOut by remember { mutableStateOf(false) }
    var alive by remember { mutableStateOf(true) }
    val loadLatest by rememberUpdatedState(load)
    val authenticateLatest by rememberUpdatedState(authenticate)
    val removeLatest by rememberUpdatedState(remove)
    val signOutLatest by rememberUpdatedState(signOut)
    val chooseLatest by rememberUpdatedState(choose)
    val scope = rememberCoroutineScope()
    DisposableEffect(Unit) { onDispose { alive = false } }
    LaunchedEffect(online, retry) {
        if (!online) return@LaunchedEffect
        loading = true
        failure = null
        try {
            val result = loadLatest()
            coroutineContext.ensureActive()
            if (alive) accounts = result.filter { it.backend == backend }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            if (alive) failure = "Couldn't load accounts: ${error.message ?: "Try again."}"
        } finally {
            if (alive && coroutineContext[kotlinx.coroutines.Job]?.isActive == true) loading = false
        }
    }
    val busy = loading || signingIn || removing != null || signingOut
    val selected = accounts.firstOrNull { it.id == selection }
    val choices = listOf("" to "Default account") + accounts.map { it.id to it.name } +
        if (!selection.isNullOrEmpty() && accounts.none { it.id == selection }) listOf(selection to "Saved account (not loaded)") else emptyList()
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Account", style = Type.body, color = Palette.text)
            Box(Modifier.weight(1f))
            Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) {
                if (busy) Spinner(12.dp)
                else IconButton(if (failure == null) "Refresh account list" else "Retry account list", "arrow.clockwise", enabled = online) { retry++ }
            }
            if (selection != null) {
                IconButton("Remove this account", "trash", enabled = online && !busy && selected != null) {
                    confirmRemove = selected
                }
            }
            ChoicePicker(selection.orEmpty(), choices, { choose(it.ifEmpty { null }) }, label = "Account")
        }
        SecondaryButton(if (signingIn) "Signing in…" else "Sign in to another account", enabled = online && !busy) {
            signingIn = true
            failure = null
            scope.launch {
                try {
                    val account = authenticateLatest()
                    coroutineContext.ensureActive()
                    if (alive) {
                        accounts = accounts.filterNot { it.id == account.id } + account
                        chooseLatest(account.id)
                    }
                } catch (cancelled: CancellationException) {
                    throw cancelled
                } catch (error: Exception) {
                    if (alive) failure = "Couldn't sign in: ${error.message ?: "Try again."}"
                } finally {
                    if (alive) signingIn = false
                }
            }
        }
        if (canSignOut) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("The computer's own sign-in is the default account.", Modifier.weight(1f),
                    style = Type.caption, color = Palette.secondary)
                SecondaryButton(if (signingOut) "Signing out…" else "Sign out default", enabled = online && !busy) {
                    confirmSignOut = true
                }
            }
        }
        val note = failure ?: if (!online) "Connect to this computer to load accounts or sign in."
            else if (signingIn) null
            else if (!loading && accounts.isEmpty()) "No isolated accounts. Use the default account or sign in to add one." else null
        if (signingIn) Text("Finish signing in in the browser on this computer.", style = Type.caption, color = Palette.secondary)
        if (note != null) Text(note, Modifier.semantics { liveRegion = LiveRegionMode.Polite },
            style = Type.caption, color = if (failure == null) Palette.secondary else Palette.warning)
        Text("Each account keeps its own sign-in on this computer. Changing accounts starts a new agent session; chat history is kept.",
            style = Type.caption, color = Palette.secondary)
    }
    val pendingRemoval = confirmRemove
    CodyncDialog(
        title = "Remove account?",
        isPresented = pendingRemoval != null,
        onDismiss = { confirmRemove = null },
        message = pendingRemoval?.let {
            "${it.name} will be signed out and its saved sign-in removed from this computer. Bots using it will need another account."
        },
        actions = {
            listOfNotNull(pendingRemoval?.let { account ->
                DialogAction("Remove", destructive = true) {
                    confirmRemove = null
                    removing = account.id
                    failure = null
                    scope.launch {
                        try {
                            removeLatest(account.id)
                            coroutineContext.ensureActive()
                            if (alive) {
                                accounts = accounts.filterNot { it.id == account.id }
                                if (selection == account.id) chooseLatest(null)
                            }
                        } catch (cancelled: CancellationException) {
                            throw cancelled
                        } catch (error: Exception) {
                            if (alive) failure = "Couldn't remove the account: ${error.message ?: "Try again."}"
                        } finally {
                            if (alive) removing = null
                        }
                    }
                }
            })
        },
    )
    CodyncDialog(
        title = "Sign out of the default account?",
        isPresented = confirmSignOut,
        onDismiss = { confirmSignOut = false },
        message = "Bots that use the computer's own sign-in will need to sign in again. Isolated accounts are not affected.",
        actions = {
            listOf(DialogAction("Sign out", destructive = true) {
                confirmSignOut = false
                signingOut = true
                failure = null
                scope.launch {
                    try {
                        signOutLatest()
                        coroutineContext.ensureActive()
                    } catch (cancelled: CancellationException) {
                        throw cancelled
                    } catch (error: Exception) {
                        if (alive) failure = "Couldn't sign out: ${error.message ?: "Try again."}"
                    } finally {
                        if (alive) signingOut = false
                    }
                }
            })
        },
    )
}
