package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
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

/**
 * An agent's other logins, beside the one the computer already has: add one (its sign-in
 * follows), sign in one that isn't yet, or remove one. Bots pick theirs in their own settings.
 */
@Composable internal fun AgentAccounts(backend: AgentBackend, host: String, store: AppStore, signIn: (String) -> Unit) {
    val scope = rememberCoroutineScope()
    // The account being removed, or [ADDING].
    var busy by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    fun run(key: String, change: suspend () -> Unit) {
        busy = key
        scope.launch {
            try { change(); error = null }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message ?: "Couldn't check this agent." }
            finally { busy = null }
        }
    }
    CardSection("Other accounts", footer = "A bot picks its account under Agent in its settings. Bots that pick none use the sign-in above.") {
        for (account in backend.accounts.orEmpty()) row {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(account.name ?: tr("New account"), style = Type.body.medium, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(tr(if (account.signedIn) "Signed in" else "Not signed in"), style = Type.subheadline, color = Palette.secondary)
                }
                if (!account.signedIn) StepAction("Sign in", enabled = busy == null) { signIn(account.id) }
                if (busy == account.id) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
                else IconButton("Remove account", "trash", enabled = busy == null) { run(account.id) { store.removeAgentAccount(account.id) } }
            }
        }
        row {
            OptionRow("Add account", "Signs in another ${backend.name} account in a terminal on $host.", "plus", busy = busy == ADDING, enabled = busy == null) {
                run(ADDING) { signIn(store.addAgentAccount(backend.id).id) }
            }
        }
        error?.let { row { ErrorRow(it) } }
    }
}
