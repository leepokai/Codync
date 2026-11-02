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
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
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
@Composable internal fun SectionHeader(header: RosterLine.Header, toggle: () -> Unit, actions: List<MenuItem>, modifier: Modifier = Modifier, drag: ContextDrag? = null, move: (Int) -> Unit = {}) {
    val translate = translator()
    val turn by animateFloatAsState(if (header.folded) -90f else 0f, Motion.layout(), label = "Chevron")
    val state = if (header.folded) tr("Collapsed") else tr("Expanded")
    // "Work, 2 bots, 3 unread", its phrase translated before the name goes in: last, so one that reads "{count}" stays as typed.
    val unread = header.folded && header.unread > 0
    val spoken = when {
        header.count == 1 && !unread -> tr("{name}, 1 bot")
        !unread -> tr("{name}, {count} bots")
        header.count == 1 -> tr("{name}, 1 bot, {unread} unread")
        else -> tr("{name}, {count} bots, {unread} unread")
    }.replace("{count}", "${header.count}").replace("{unread}", "${header.unread}").replace("{name}", header.name)
    val tap = if (actions.isEmpty() && drag == null) Modifier.clickable(role = Role.Button, onClick = toggle)
        else Modifier.contextActions(onClick = toggle, reorder = drag) { actions }.semantics { role = Role.Button }
    Row(modifier.fillMaxWidth().padding(top = 14.dp, bottom = 2.dp).then(tap)
        // One spoken line for the whole header (the count and the badge say nothing alone): clear the parts, then describe it.
        .clearAndSetSemantics {
            heading()
            customActions = listOf(CustomAccessibilityAction(translate("Move up")) { move(-1); true }, CustomAccessibilityAction(translate("Move down")) { move(1); true })
            stateDescription = state
            contentDescription = spoken
        }
        .padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(header.name, Modifier.weight(1f, fill = false), style = Type.footnote.semibold, color = Palette.secondary,
                maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text("${header.count}", style = Type.footnote, color = Palette.tertiary)
        }
        Spacer(Modifier.width(8.dp))
        // The badge sits at the trailing edge, next to the chevron, as on iPhone and the desktop.
        AnimatedVisibility(header.folded && header.unread > 0, Modifier.padding(end = 6.dp), enter = fadeIn(Motion.fade()), exit = fadeOut(Motion.fade())) {
            UnreadBadge(header.unread)
        }
        Symbol("chevron.down", Modifier.rotate(turn), size = 14.dp, tint = Palette.tertiary)
    }
}

/** The plain "Bots" label over the bots in no section: a header's name, where its row starts; it folds nothing. */
@Composable internal fun SectionLabel(modifier: Modifier = Modifier, drag: ContextDrag? = null, move: (Int) -> Unit = {}) {
    val translate = translator()
    Text(tr("Bots"), modifier.fillMaxWidth().padding(top = 14.dp, bottom = 2.dp).contextActions(reorder = drag) { emptyList() }.semantics {
        heading()
        customActions = listOf(CustomAccessibilityAction(translate("Move up")) { move(-1); true }, CustomAccessibilityAction(translate("Move down")) { move(1); true })
    }.padding(vertical = 6.dp),
        style = Type.footnote.semibold, color = Palette.secondary)
}

/**
 * The row's "Move to section" submenu: each other section there is (by the name the user gave it, never translated), a new one,
 * and none when the bot is in one; [move] puts the bot in a section (null: out of its own).
 */
internal fun sectionMenu(bot: Bot, sections: List<String>, move: (String?) -> Unit, newSection: () -> Unit): MenuItem {
    val others = sections.filter { it != bot.section }.map { name -> MenuItem(name, icon = "tray", verbatim = true) { move(name) } }
    return MenuItem("Move to section", icon = "tray", children = others + listOfNotNull(
        MenuItem("New section…", icon = "plus", divider = others.isNotEmpty(), action = newSection),
        if (bot.section != null) MenuItem("Remove from section", icon = "minus.circle") { move(null) } else null,
    ))
}

/** The header's menu: rename the section, or remove it (its bots stay, without a section). */
internal fun sectionActions(rename: () -> Unit, remove: () -> Unit): List<MenuItem> = listOf(
    MenuItem("Rename section…", icon = "pencil", action = rename),
    MenuItem("Remove section", icon = "xmark.circle", action = remove),
)

/** An open section name dialog (desktop `SectionNaming`): its title, the name it opens with, and what Save does with the typed one. */
internal class SectionNaming(val title: String, val name: String, val save: (String) -> Unit)

/**
 * "New section" / "Rename section": a name, then Save. A name of nothing saves nothing; the host says what else it refuses.
 * Each opening ([naming] is a new object every time) starts from its own name; the last one stays on show while the dialog fades out.
 */
@Composable internal fun SectionNameDialog(naming: SectionNaming?, dismiss: () -> Unit) {
    var opened by remember { mutableStateOf<SectionNaming?>(null) }
    var name by remember { mutableStateOf(TextFieldValue()) }
    if (naming != null && naming !== opened) {
        opened = naming
        name = TextFieldValue(naming.name, TextRange(naming.name.length))
    }
    val submit = { if (naming != null) sectionName(name.text)?.let(naming.save) }
    val focus = remember { FocusRequester() }
    CodyncDialog(opened?.title.orEmpty(), naming != null, dismiss, field = {
        LaunchedEffect(Unit) { focus.requestFocus() }
        PlainTextField(name, { name = it.copy(text = limitSectionName(it.text)) }, Modifier.fillMaxWidth().pill().focusRequester(focus),
            "Section name", singleLine = true, keyboard = KeyboardOptions(capitalization = KeyboardCapitalization.Words, imeAction = ImeAction.Done),
            actions = KeyboardActions(onDone = { dismiss(); submit() }))
    }) { listOf(DialogAction("Save", action = submit)) }
}
