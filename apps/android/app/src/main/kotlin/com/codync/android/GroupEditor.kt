package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.animateContentSize
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.text.style.TextOverflow
import com.codync.android.core.Bot
import com.codync.android.core.GroupDraft
import com.codync.android.design.*

/**
 * Change a group chat's name, what it's for and who's in it (iOS `GroupEditorView`); new groups start in [NewGroupView].
 * Bots are picked like recipients: members on top, a search, and the bots not in it yet.
 */
@Composable internal fun GroupEditor(state: AppState, initial: GroupDraft, store: AppStore) {
    val dismiss = LocalDismissModal.current
    var draft by remember(initial.id) { mutableStateOf(initial) }
    var query by remember { mutableStateOf("") }
    var adding by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }
    val saving = "editor" in state.busy
    val picked = draft.members.mapNotNull { id -> state.bots.firstOrNull { it.id == id } }
    val candidates = roster(state.bots).filter { it.kind != "group" && it.id !in draft.members && it.name.contains(query.trim(), ignoreCase = true) }
    val defaultName = picked.joinToString(", ") { it.name }.ifEmpty { "Name" }
    fun toggle(id: String) {
        draft = draft.copy(members = if (id in draft.members) draft.members - id else draft.members + id)
        query = ""
    }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("Group chat") {
            if (saving) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
            else IconButton("Save", "checkmark", enabled = draft.members.isNotEmpty()) {
                store.saveGroup(draft) { dismiss() }
            }
        }
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(28.dp)) {
            Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
                GroupAvatar(picked.map { AvatarFace(it.avatarShape, it.avatarColor, it.status) }, 84.dp)
                PlainTextField(draft.name, { draft = draft.copy(name = it) }, Modifier.fillMaxWidth().padding(top = 6.dp), defaultName, singleLine = true,
                    style = Type.title2.semibold.copy(textAlign = TextAlign.Center))
                PlainTextField(draft.description, { draft = draft.copy(description = it) }, Modifier.fillMaxWidth(), "What this group works on",
                    style = Type.subheadline.copy(textAlign = TextAlign.Center, color = Palette.secondary), maxLines = 4)
            }
            Column(Modifier.fillMaxWidth().animateContentSize(Motion.layoutSize)) {
                Text("Members", Modifier.padding(bottom = 4.dp), style = Type.subheadline.semibold, color = Palette.secondary)
                for (bot in picked) MemberRow(bot, state.bots) { IconButton("Remove ${bot.name}", "xmark") { toggle(bot.id) } }
                if (adding) {
                    Box(Modifier.padding(vertical = 6.dp)) {
                        FieldBox(query, { query = it }, "Search bots", fill = Palette.surface, imeAction = ImeAction.Done,
                            actions = KeyboardActions(onDone = { candidates.firstOrNull()?.let { toggle(it.id) } }))
                    }
                    for (bot in candidates) Box(Modifier.plainButton { toggle(bot.id) }.semantics(mergeDescendants = true) { contentDescription = "Add ${bot.name}" }) {
                        MemberRow(bot, state.bots, dimmed = true) { Symbol("plus.circle", size = 20.dp, tint = Palette.secondary) }
                    }
                } else if (candidates.isNotEmpty()) Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).plainButton { adding = true },
                    verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    Box(Modifier.width(32.dp), contentAlignment = Alignment.Center) { Symbol("plus", size = 18.dp, tint = Palette.secondary) }
                    Text("Add Member", style = Type.body, color = Palette.secondary)
                }
                Text("Everyone answers in turn unless you @mention someone.", Modifier.padding(top = 10.dp), style = Type.caption, color = Palette.tertiary)
            }
            AnimatedVisibility(state.error != null, enter = fadeIn(Motion.fade()), exit = fadeOut(Motion.fade())) {
                Text(state.error.orEmpty(), style = Type.footnote, color = Palette.danger)
            }
            LinkButton("Delete group chat", color = Palette.danger) { confirmDelete = true }
        }
    }
    val group = state.bots.firstOrNull { it.id == draft.id }
    CodyncDialog("Delete ${group?.name ?: "group chat"}?", confirmDelete, { confirmDelete = false }, "Its bots and their own chats stay.") {
        listOf(DialogAction("Delete group chat", destructive = true) { group?.let { store.deleteBot(it) { dismiss() } } })
    }
}

/** A member or candidate: avatar, name, and a trailing control. */
@Composable private fun MemberRow(bot: Bot, bots: List<Bot>, dimmed: Boolean = false, trailing: @Composable () -> Unit) {
    Row(Modifier.fillMaxWidth().heightIn(min = 48.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
        BotAvatar(bot, bots, 32.dp)
        Text(bot.name, Modifier.weight(1f), style = Type.body, color = if (dimmed) Palette.secondary else Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        trailing()
    }
}
