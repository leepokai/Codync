package com.codync.android

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.*
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import kotlinx.coroutines.delay
import java.io.IOException

/**
 * What a bot's chat runs on, in its message box (iOS `ModelChip`, the desktop's `BotRunOptions`): the agent's logo and
 * the model. It opens the agent and model picker, whose rows are the installed agents' logins; each change is saved to
 * the bot a moment after it's made, as its settings are (a typed model ID isn't saved per keystroke: each save starts a
 * new agent session). The chip, not its sheet, keeps the draft and the save, so closing the sheet right after a pick
 * still saves it. Saving needs the computer, as the settings' Save does.
 */
@Composable internal fun ModelChip(bot: Bot, state: AppState, store: AppStore) {
    var open by remember { mutableStateOf(false) }
    val online = state.actualConnection is LinkState.Ready
    val saved = BotDraft.from(bot)
    var draft by rememberBotDraft(saved, state)
    var saveError by remember(bot.id) { mutableStateOf<String?>(null) }
    // Keyed on the saved bot too (`BotDetailsView`): an edit made while the previous save was still out (and so skipped)
    // is sent once that save lands.
    LaunchedEffect(draft, online, saved) {
        if (!online || !draft.isValid || draft == saved) return@LaunchedEffect
        delay(600)
        store.saveSettings(draft) { error -> if (open) saveError = error else error?.let { store.showError(IOException(it)) } }
    }
    Row(Modifier.height(34.dp).clip(Capsule()).clickable(enabled = online, role = Role.Button) { open = true }
        .semantics { contentDescription = "Model" }.padding(horizontal = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
        AgentIcon(agentRegistry(state, bot.backend), 14.dp)
        Text(bot.model ?: tr("Default"), style = Type.subheadline, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Symbol("chevron.down", size = 9.dp, tint = Palette.tertiary)
    }
    CodyncSheet(open, { open = false }) { ModelSheet(bot, draft, { draft = it }, saveError, state, store) }
}

/** The bot's agent, login and model. */
@Composable private fun ModelSheet(bot: Bot, draft: BotDraft, edit: (BotDraft) -> Unit, error: String?, state: AppState, store: AppStore) {
    Column(Modifier.fillMaxSize()) {
        ModalHeader("Model")
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            AgentModelPicker(state, store::agentModels, draft.backend, draft.model, draft.modelOptions, draft.fastMode ?: false, bot,
                { model, options -> edit(draft.copy(model = model, modelOptions = options)) }, { edit(draft.copy(fastMode = it)) },
                agents = readySeats(state, draft.login), chooseAgent = { edit(draft.withAgent(it)) }, expanded = true, loginProfile = draft.loginProfile) {
                bot.restOf(draft.login)?.let { Text(tr(it.line(RelativeTime.clock(it.until))), style = Type.caption, color = Palette.secondary) }
            }
            error?.let { Text(tr(it), style = Type.footnote, color = Palette.danger) }
        }
    }
}
