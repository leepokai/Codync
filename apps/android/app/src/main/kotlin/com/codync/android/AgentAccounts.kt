package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.AgentBackend
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

private const val ADDING = "add"
private const val OWN = "own"
private const val NAME_LIMIT = 60

/** The computer's own login, as the agent's setup knows it: its sign-in state and how to ask again. */
internal class OwnLogin(val status: String, val checking: Boolean, val check: () -> Unit)

/**
 * An agent's accounts in one place: the default one (the login the computer already has, with its sign-in options as
 * [ownRows]) and the others. Each can be named; others are added (their sign-in follows), signed in or removed. Bots
 * pick theirs in their own settings. [signIn] opens the sign-in terminal for an account; [signInNow] is an account
 * whose sign-in was asked for (a chat's sign-in card), started here ([signInStarted]) when its agent signs itself in.
 */
@Composable internal fun AgentAccounts(backend: AgentBackend, host: String, store: AppStore, own: OwnLogin, signIn: (String) -> Unit,
    signInNow: String?, signInStarted: () -> Unit, ownRows: CardRows.() -> Unit) {
    val scope = rememberCoroutineScope()
    // The account being changed, [OWN] or [ADDING].
    var busy by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var naming by remember { mutableStateOf<Naming?>(null) }
    fun run(key: String, change: suspend () -> Unit) {
        busy = key
        scope.launch {
            try { change(); error = null }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message ?: "Couldn't check this agent." }
            finally { busy = null }
        }
    }
    // Its agent signs the account in itself (a browser on the computer), or its CLI does in a terminal.
    val inBrowser = backend.signsInBrowser
    suspend fun start(account: String) = if (inBrowser) store.signInAgentAccount(backend.id, account) else signIn(account)
    fun rename(account: String?, label: String?) {
        naming = Naming("Rename account", label.orEmpty()) { run(account ?: OWN) { store.renameAgentAccount(backend.id, account, it) } }
    }
    LaunchedEffect(Unit) {
        if (signInNow == null || !inBrowser) return@LaunchedEffect
        signInStarted()
        if (backend.accounts?.firstOrNull { it.id == signInNow }?.signedIn == false) run(signInNow) { start(signInNow) }
    }

    val mine = backend.ownAccount
    val defaultName = mine?.name ?: tr("Default account")
    val defaultDetail = listOfNotNull(tr("Default"), mine?.email?.takeIf { mine.label != null }, tr(own.status))
    val unnamed = tr("New account")
    val signedIn = tr("Signed in")
    val signedOut = tr("Not signed in")
    val finishing = tr("Finish signing in in the browser on $host…")
    CardSection("Accounts", footer = "A bot picks its account under Agent in its settings. Bots that pick none use the default account.") {
        row {
            AccountLine(defaultName, defaultDetail, { rename(null, mine?.label) }) {
                CheckAgain(own.checking, own.check)
            }
        }
        ownRows()
        for (account in backend.accounts.orEmpty()) row {
            val state = if (busy == account.id && !account.signedIn && inBrowser) finishing else if (account.signedIn) signedIn else signedOut
            AccountLine(account.name ?: unnamed, listOfNotNull(account.email?.takeIf { account.label != null }, state), { rename(account.id, account.label) }) {
                if (!account.signedIn) StepAction("Sign in", enabled = busy == null) { run(account.id) { start(account.id) } }
                if (busy == account.id) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
                else IconButton("Remove account", "trash", enabled = busy == null) { run(account.id) { store.removeAgentAccount(account.id) } }
            }
        }
        row {
            OptionRow("Add account", "Signs in another ${backend.name} account in a ${if (inBrowser) "browser" else "terminal"} on $host.", "plus",
                busy = busy == ADDING, enabled = busy == null) {
                run(ADDING) { start(store.addAgentAccount(backend.id).id) }
            }
        }
        error?.let { row { ErrorRow(it) } }
    }
    NameDialog(naming, "Account name", { naming = null }, { it.take(NAME_LIMIT) }) { it.trim() }
}

/** One account: its name, what's known of it, the pencil that renames it and its other actions. */
@Composable private fun AccountLine(name: String, detail: List<String>, rename: () -> Unit, actions: @Composable RowScope.() -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(name, style = Type.body.medium, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            SelectionContainer { Text(detail.joinToString(" · "), style = Type.subheadline, color = Palette.secondary) }
        }
        IconButton("Rename account", "pencil", action = rename)
        actions()
    }
}
