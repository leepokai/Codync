package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.AgentAccount
import com.codync.android.core.AgentBackend
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/**
 * The accounts an agent keeps on this computer (iOS `AgentAccountsSection`): the computer's
 * own sign-in and the isolated profiles bots can pick. Account sign-in can take a few minutes.
 */
@Composable internal fun AgentAccountsSection(backend: AgentBackend, state: AppState, store: AppStore, onChanged: () -> Unit) {
    val scope = rememberCoroutineScope()
    val host = state.hostName
    var accounts by remember { mutableStateOf(emptyList<AgentAccount>()) }
    var loading by remember { mutableStateOf(true) }
    var adding by remember { mutableStateOf(false) }
    var removing by remember { mutableStateOf<String?>(null) }
    var disconnecting by remember { mutableStateOf(false) }
    var failure by remember { mutableStateOf<String?>(null) }
    var retry by remember { mutableIntStateOf(0) }
    var confirmRemove by remember { mutableStateOf<AgentAccount?>(null) }
    var confirmSignOut by remember { mutableStateOf(false) }
    val busy = loading || adding || removing != null || disconnecting
    LaunchedEffect(backend.id, retry) {
        loading = true
        failure = null
        try {
            accounts = store.agentAccounts(backend.id)
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Exception) {
            failure = "Couldn't load accounts: ${error.message ?: "Try again."}"
        } finally {
            loading = false
        }
    }
    val defaultDetail = when (backend.signedIn) {
        true -> backend.email?.let { "Signed in as $it on $host." } ?: "Signed in on $host."
        false -> "Not signed in on $host."
        null -> "Couldn't tell whether $host is signed in."
    }
    CardSection("Accounts", footer = "Each account keeps its own sign-in on $host. Bots can use the default account or any isolated one.") {
        row {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text("Default account", style = Type.body.medium, color = Palette.text)
                    Text(defaultDetail, style = Type.subheadline, color = Palette.secondary)
                }
                if (backend.signOut == true) {
                    SecondaryButton(if (disconnecting) "Signing out…" else "Disconnect", enabled = !busy) { confirmSignOut = true }
                }
            }
        }
        if (loading) row {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Spinner(12.dp)
                Text("Loading accounts…", style = Type.subheadline, color = Palette.secondary)
            }
        }
        for ((index, account) in accounts.withIndex()) row {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text("${backend.name} ${index + 2}", style = Type.body.medium, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(account.email?.let { "Signed in as $it on $host." } ?: "Isolated account on $host.",
                        style = Type.subheadline, color = Palette.secondary)
                }
                IconButton("Remove ${backend.name} ${index + 2}", "trash", enabled = !busy) { confirmRemove = account }
            }
        }
        row {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text("Add account", style = Type.body.medium, color = Palette.text)
                    Text(if (adding) "Finish signing in in the browser on $host…"
                         else "Sign in to another account without signing out of this one.",
                        style = Type.subheadline, color = Palette.secondary, maxLines = 3, overflow = TextOverflow.Ellipsis)
                }
                SecondaryButton(if (adding) "Signing in…" else "+ Add account", enabled = !busy) {
                    adding = true
                    failure = null
                    scope.launch {
                        try {
                            val account = store.authenticateAgentAccount(backend.id)
                            accounts = accounts.filterNot { it.id == account.id } + account
                            onChanged()
                        } catch (cancelled: CancellationException) {
                            throw cancelled
                        } catch (error: Exception) {
                            failure = "Couldn't add the account: ${error.message ?: "Try again."}"
                        } finally {
                            adding = false
                        }
                    }
                }
            }
        }
        failure?.let { message -> row {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(message, Modifier.semantics { liveRegion = LiveRegionMode.Polite }, style = Type.caption, color = Palette.warning)
                SecondaryButton("Retry loading accounts", enabled = !busy) { retry++ }
            }
        } }
    }
    val pendingRemoval = confirmRemove
    CodyncDialog(
        title = "Remove account?",
        isPresented = pendingRemoval != null,
        onDismiss = { confirmRemove = null },
        message = pendingRemoval?.let {
            "${it.name} will be signed out and its saved sign-in removed from $host. Bots using it will need another account."
        },
        actions = {
            listOfNotNull(pendingRemoval?.let { account ->
                DialogAction("Remove", destructive = true) {
                    confirmRemove = null
                    removing = account.id
                    failure = null
                    scope.launch {
                        try {
                            store.deleteAgentAccount(account.id)
                            accounts = accounts.filterNot { it.id == account.id }
                            onChanged()
                        } catch (cancelled: CancellationException) {
                            throw cancelled
                        } catch (error: Exception) {
                            failure = "Couldn't remove the account: ${error.message ?: "Try again."}"
                        } finally {
                            removing = null
                        }
                    }
                }
            })
        },
    )
    CodyncDialog(
        title = "Sign out of ${backend.name}?",
        isPresented = confirmSignOut,
        onDismiss = { confirmSignOut = false },
        message = "This removes the computer's own sign-in on $host, as if you no longer have this account. Isolated accounts are not affected.",
        actions = {
            listOf(DialogAction("Sign out", destructive = true) {
                confirmSignOut = false
                disconnecting = true
                failure = null
                scope.launch {
                    try {
                        store.signOutAgent(backend.id)
                        onChanged()
                    } catch (cancelled: CancellationException) {
                        throw cancelled
                    } catch (error: Exception) {
                        failure = "Couldn't sign out: ${error.message ?: "Try again."}"
                    } finally {
                        disconnecting = false
                    }
                }
            })
        },
    )
}
