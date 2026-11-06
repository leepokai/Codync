package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
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
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.BotDraft
import com.codync.android.core.GroupDraft
import com.codync.android.core.LinkState
import com.codync.android.design.*

/** A new bot from its face and name (iOS `NewBotView`); the rest starts on the defaults and changes later in its Settings. */
/** [computers] (ID, label) can take the bot; with more than one it can move to another before it's created ([choose]). */
@Composable fun NewBotView(state: AppState, store: AppStore, computers: List<Pair<String, String>> = emptyList(), choose: (String) -> Unit = {}) {
    var draft by remember { mutableStateOf(BotDraft()) }
    val online = state.actualConnection is LinkState.Ready
    val dismiss = LocalDismissModal.current
    // A new bot starts on an installed agent we know (again on another computer); the host turns on every connector and skill.
    LaunchedEffect(state.computer.id, state.hello) { draft = fillDefaults(draft, state) }
    CreateLayout(state, "New bot", draft.name, { draft = draft.copy(name = it) }, "Name your bot", online, { store.saveBot(draft) { dismiss() } },
        { CharacterAvatar(draft.avatarShape, draft.avatarColor, 96.dp) }) {
        AvatarPicker(draft.avatarShape, draft.avatarColor, { draft = draft.copy(avatarShape = it) }, { draft = draft.copy(avatarColor = it) })
        if (computers.size > 1) ValueRow("Computer") { ChoicePicker(state.computer.id, computers, choose, label = "Computer") }
        if (!online) Text(if (computers.size > 1) "Connect this computer to create the bot, or choose another computer."
            else "Connect this computer to create the bot.", style = Type.caption, color = Palette.warning)
    }
}

/** A new group chat from the bots it brings together (iOS `NewGroupView`); what it works on is set later in its editor. */
@Composable fun NewGroupView(state: AppState, store: AppStore) {
    var name by remember { mutableStateOf("") }
    var members by remember { mutableStateOf(emptyList<String>()) }
    val dismiss = LocalDismissModal.current
    val bots = roster(state.bots).filter { it.kind != "group" }
    val picked = members.mapNotNull { id -> bots.firstOrNull { it.id == id } }
    CreateLayout(state, "New group chat", name, { name = it }, picked.joinToString(", ") { it.name }.ifEmpty { "Name your group" }, picked.isNotEmpty(),
        { store.saveGroup(GroupDraft(name = name, members = picked.map(Bot::id))) { dismiss() } },
        { GroupAvatar(picked.map { AvatarFace(it.avatarShape, it.avatarColor, it.status) }, 96.dp) }) {
        if (bots.isEmpty()) Text("No bots yet. Create a bot first.", style = Type.subheadline, color = Palette.secondary)
        else ChoiceGrid(8.dp) {
            for (bot in bots) MemberTile(bot, state.bots, bot.id in members) { members = if (bot.id in members) members - bot.id else members + bot.id }
        }
        Text("Everyone answers in turn unless you @mention someone.", style = Type.caption, color = Palette.tertiary)
    }
}

/** Both create dialogs' layout (iOS `CreateForm`): a big preview, the centered name (Done creates), the grids, then any save error. */
@Composable private fun CreateLayout(state: AppState, title: String, name: String, rename: (String) -> Unit, prompt: String, canCreate: Boolean,
    create: () -> Unit, preview: @Composable () -> Unit, content: @Composable ColumnScope.() -> Unit) {
    val saving = "editor" in state.busy
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(title) {
            if (saving) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
            else IconButton("Create", "checkmark", enabled = canCreate, action = create)
        }
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 24.dp),
            horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(24.dp)) {
            preview()
            PlainTextField(name, rename, Modifier.fillMaxWidth().focusRequester(focus), prompt, singleLine = true,
                style = Type.title2.semibold.copy(textAlign = TextAlign.Center),
                keyboard = KeyboardOptions(capitalization = KeyboardCapitalization.Words, imeAction = ImeAction.Done),
                actions = KeyboardActions(onDone = { if (canCreate && !saving) create() }))
            content()
            AnimatedVisibility(state.error != null, enter = fadeIn(Motion.fade()), exit = fadeOut(Motion.fade())) {
                Text(state.error.orEmpty(), style = Type.footnote, color = Palette.danger)
            }
        }
    }
}

/** A bot to pick for the group: its avatar, ringed when picked, over its name. */
@Composable private fun MemberTile(bot: Bot, bots: List<Bot>, picked: Boolean, toggle: () -> Unit) {
    Column(Modifier.width(64.dp).choice(picked, bot.name, Role.Checkbox, toggle), horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Box(Modifier.ring(picked)) { BotAvatar(bot, bots, 44.dp) }
        Text(bot.name, style = Type.caption, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** Picks an installed agent we know (a registry agent nobody chose is never the default). */
private fun fillDefaults(draft: BotDraft, state: AppState): BotDraft {
    val backends = backends(state)
    if (backends.any { it.text("id") == draft.backend && it.available }) return draft
    return backends.firstOrNull { it.available && it.curated }?.text("id")?.let { draft.copy(backend = it) } ?: draft
}
