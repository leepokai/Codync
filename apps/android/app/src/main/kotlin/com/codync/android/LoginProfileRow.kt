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
import com.codync.android.core.loginProfileChoices
import com.codync.android.design.*

/** "Login profile": only where the host sends the agent's `loginProfiles` (older hosts don't). */
internal fun CardRows.loginProfileRow(state: AppState, store: AppStore, draft: BotDraft, change: (BotDraft) -> Unit) {
    val backend = agentBackends(state).firstOrNull { it.id == draft.backend }?.takeIf { it.loginProfiles != null } ?: return
    row { LoginProfilePicker(backend, draft.loginProfile, state, store) { change(draft.copy(loginProfile = it)) } }
}

/**
 * Which of the agent's login profiles a bot runs on: the default one (the login the computer already has), or one of its others.
 * "Add login profile…" opens the agent's setup, where a new one is added. [free]: the login profiles that can be picked
 * (null: the default one), when not all can: a fallback's, whose chain has the others.
 */
@Composable internal fun LoginProfilePicker(backend: AgentBackend, loginProfile: String?, state: AppState, store: AppStore, free: List<String?>? = null,
    select: (String?) -> Unit) {
    var adding by remember { mutableStateOf(false) }
    val choices = backend.loginProfileChoices(loginProfile).filter { free == null || it.id in free }
    val own = backend.defaultLoginProfile?.name ?: tr("Default login profile")
    val unnamed = tr("New login profile")
    val current = if (loginProfile == null) own else choices.firstOrNull { it.id == loginProfile }?.name ?: unnamed
    ValueRow("Login profile") {
        DropdownMenu({
            listOfNotNull(MenuItem(own, selected = loginProfile == null, verbatim = true) { select(null) }.takeIf { free == null || null in free }) +
                choices.map { MenuItem(it.name ?: unnamed, selected = it.id == loginProfile, verbatim = true) { select(it.id) } } +
                MenuItem("Add login profile…", divider = true) { adding = true }
        }, label = "${tr("Login profile")}: $current") {
            Row(Modifier.outline().pill(Color.Transparent), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(current, Modifier.weight(1f, fill = false), style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Symbol("chevron.down", size = 13.dp, tint = Palette.secondary)
            }
        }
    }
    CodyncSheet(adding, { adding = false }) { AgentSheet(backend, state, store) }
}
