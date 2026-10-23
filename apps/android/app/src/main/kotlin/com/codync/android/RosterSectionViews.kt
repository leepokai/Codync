package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.design.*

/**
 * A section's heading, drawn like iOS `ComputerSection`: its name and bot count, their unread while
 * folded, and the chevron. Tap folds it; touch and hold opens [actions] (none for a host without sections).
 */
@Composable internal fun SectionHeader(header: RosterLine.Header, toggle: () -> Unit, actions: List<MenuItem>, modifier: Modifier = Modifier) {
    val turn by animateFloatAsState(if (header.folded) -90f else 0f, Motion.layout(), label = "Chevron")
    val tap = if (actions.isEmpty()) Modifier.clickable(role = Role.Button, onClick = toggle)
        else Modifier.contextActions(onClick = toggle) { actions }.semantics { role = Role.Button }
    Row(modifier.fillMaxWidth().padding(top = 14.dp, bottom = 2.dp).then(tap)
        // One spoken line for the whole header (the count and the badge say nothing alone): clear the parts, then describe it.
        .clearAndSetSemantics {
            heading()
            stateDescription = if (header.folded) "Collapsed" else "Expanded"
            contentDescription = buildString {
                append("${header.name}, ${header.count} ${if (header.count == 1) "bot" else "bots"}")
                if (header.folded && header.unread > 0) append(", ${header.unread} unread")
            }
        }
        .padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(header.name, Modifier.weight(1f, fill = false), style = Type.footnote.semibold, color = Palette.secondary,
                maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text("${header.count}", style = Type.footnote, color = Palette.tertiary)
            AnimatedVisibility(header.folded && header.unread > 0, enter = fadeIn(Motion.fade()), exit = fadeOut(Motion.fade())) {
                UnreadBadge(header.unread)
            }
        }
        Spacer(Modifier.width(8.dp))
        Symbol("chevron.down", Modifier.rotate(turn), size = 14.dp, tint = Palette.tertiary)
    }
}

/** The plain "Bots" label over the bots in no section: a header's name, where its row starts; it folds nothing. */
@Composable internal fun SectionLabel(modifier: Modifier = Modifier) {
    Text("Bots", modifier.fillMaxWidth().padding(top = 14.dp, bottom = 2.dp).semantics { heading() }.padding(vertical = 6.dp),
        style = Type.footnote.semibold, color = Palette.secondary)
}

/** The row's "Move to section" submenu: each other section there is, a new one, and none when the bot is in one. */
internal fun sectionMenu(bot: Bot, sections: List<String>, store: AppStore, newSection: () -> Unit): MenuItem {
    val others = sections.filter { it != bot.section }.map { name -> MenuItem(name) { store.setSection(bot.id, name) } }
    return MenuItem("Move to section", icon = "arrow.turn.down.right", children = others + listOfNotNull(
        MenuItem("New section…", icon = "plus", divider = true, action = newSection),
        if (bot.section != null) MenuItem("Remove from section", icon = "xmark.circle") { store.setSection(bot.id, null) } else null,
    ))
}

/** The header's menu: rename the section, or remove it (its bots stay, without a section). */
internal fun sectionActions(rename: () -> Unit, remove: () -> Unit): List<MenuItem> = listOf(
    MenuItem("Rename section…", icon = "pencil", action = rename),
    MenuItem("Remove section", icon = "xmark.circle", action = remove),
)

/** What the name dialog names: a new section for a bot, or a section to rename. */
internal sealed interface SectionNaming {
    data class New(val botId: String) : SectionNaming
    data class Rename(val section: String) : SectionNaming
}

/**
 * "New section" / "Rename section": a name, then Save. A name of nothing saves nothing; the host says what else it refuses.
 * Each opening ([naming] is a new object every time) starts from its own name; the last one stays on show while the dialog fades out.
 */
@Composable internal fun SectionNameDialog(naming: SectionNaming?, store: AppStore, rename: (from: String, to: String) -> Unit, dismiss: () -> Unit) {
    var opened by remember { mutableStateOf<SectionNaming?>(null) }
    var name by remember { mutableStateOf(TextFieldValue()) }
    if (naming != null && naming !== opened) {
        opened = naming
        val start = (naming as? SectionNaming.Rename)?.section.orEmpty()
        name = TextFieldValue(start, TextRange(start.length))
    }
    val submit = { if (naming != null) sectionName(name.text)?.let { store.nameSection(naming, it, rename) } }
    val focus = remember { FocusRequester() }
    CodyncDialog(if (opened is SectionNaming.Rename) "Rename section" else "New section", naming != null, dismiss, field = {
        LaunchedEffect(Unit) { focus.requestFocus() }
        PlainTextField(name, { name = it.copy(text = limitSectionName(it.text)) }, Modifier.fillMaxWidth().pill().focusRequester(focus),
            "Section name", singleLine = true, keyboard = KeyboardOptions(capitalization = KeyboardCapitalization.Words, imeAction = ImeAction.Done),
            actions = KeyboardActions(onDone = { dismiss(); submit() }))
    }) { listOf(DialogAction("Save", action = submit)) }
}

private fun AppStore.nameSection(naming: SectionNaming, name: String, rename: (from: String, to: String) -> Unit) {
    when (naming) {
        is SectionNaming.New -> setSection(naming.botId, name)
        is SectionNaming.Rename -> rename(naming.section, name)
    }
}
