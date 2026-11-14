package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.AgentBackend
import com.codync.android.core.BotDraft
import com.codync.android.core.accountChoices
import com.codync.android.design.*

/** "Account": only where the bot's agent can have other logins (the host sends its `accounts`). */
internal fun CardRows.accountRow(state: AppState, store: AppStore, draft: BotDraft, change: (BotDraft) -> Unit) {
    val backend = agentBackends(state).firstOrNull { it.id == draft.backend }?.takeIf { it.accounts != null } ?: return
    row { AccountPicker(backend, draft.account, state, store) { change(draft.copy(account = it)) } }
}

/**
 * Which of the agent's logins a bot runs on: the one the computer already has, or one of its other
 * accounts. "Add account…" opens the agent's setup, where a new one signs in.
 */
@Composable private fun AccountPicker(backend: AgentBackend, account: String?, state: AppState, store: AppStore, select: (String?) -> Unit) {
    var adding by remember { mutableStateOf(false) }
    val choices = backend.accountChoices(account)
    val own = tr("This computer's sign-in")
    val unnamed = tr("New account")
    val current = if (account == null) own else choices.firstOrNull { it.id == account }?.name ?: unnamed
    ValueRow("Account") {
        DropdownMenu({
            listOf(MenuItem(own, selected = account == null, verbatim = true) { select(null) }) +
                choices.map { MenuItem(it.name ?: unnamed, selected = it.id == account, verbatim = true) { select(it.id) } } +
                MenuItem("Add account…", divider = true) { adding = true }
        }, label = "${tr("Account")}: $current") {
            Row(Modifier.outline().pill(Color.Transparent), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(current, Modifier.weight(1f, fill = false), style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Symbol("chevron.down", size = 13.dp, tint = Palette.secondary)
            }
        }
    }
    CodyncSheet(adding, { adding = false }) { AgentSheet(backend, state, store) }
}
