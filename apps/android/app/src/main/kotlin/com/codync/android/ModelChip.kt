package com.codync.android

import androidx.compose.foundation.clickable
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
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
    ModelChipLabel(agentRegistry(state, bot.backend), bot.model, enabled = online) { open = true }
    CodyncSheet(open, { open = false }) { ModelSheet(bot, draft, { draft = it }, saveError, state, store) }
}

/** The model chip itself: the agent's logo ([registry]) and [model] (null: the agent's default); [open] opens its picker. */
@Composable internal fun ModelChipLabel(registry: String, model: String?, modifier: Modifier = Modifier, enabled: Boolean = true, open: () -> Unit) {
    ChipLabel(modifier.clip(Capsule()).clickable(enabled = enabled, role = Role.Button, onClick = open).semantics { contentDescription = "Model" }) {
        AgentIcon(registry, 14.dp)
        Text(model ?: tr("Default"), style = Type.subheadline, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** The bot's agent, login and model. */
@Composable private fun ModelSheet(bot: Bot, draft: BotDraft, edit: (BotDraft) -> Unit, error: String?, state: AppState, store: AppStore) {
    val notes = buildList {
        bot.restOf(draft.login)?.let { add(tr(it.line(RelativeTime.clock(it.until))) to Palette.secondary) }
        add(tr("Changing the model starts a new agent session. Chat history is kept.") to Palette.secondary)
        error?.let { add(tr(it) to Palette.danger) }
    }
    ModelPanel(state, store, backend = draft.backend, loginProfile = draft.loginProfile, model = draft.model, options = draft.modelOptions,
        choose = { model, options -> edit(draft.copy(model = model, modelOptions = options)) }, fast = draft.fastMode ?: false,
        setFast = { edit(draft.copy(fastMode = it)) }, bot = bot, seats = readySeats(state, draft.login), pickSeat = { edit(draft.withAgent(it)) },
        notes = notes, approvals = draft, setApprovals = edit)
}
