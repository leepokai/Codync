package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle

/** A list above the box: hugs its rows, scrolls past 280 dp (iOS `ComposerList`). */
@Composable private fun ComposerList(modifier: Modifier, rows: @Composable () -> Unit) {
    Column(modifier.fillMaxWidth().heightIn(max = 280.dp).glass(RoundedRectangle(20.dp)).verticalScroll(rememberScrollState())
        .padding(vertical = 6.dp)) { rows() }
}

/** A row of a list above the box: its icon, name and what it is; tapping it picks it (iOS `ComposerListRow`). */
@Composable private fun ComposerListRow(title: String, detail: String, pick: () -> Unit, warning: Boolean = false, icon: @Composable () -> Unit) {
    Row(Modifier.fillMaxWidth().plainButton(action = pick)
        .semantics(mergeDescendants = true) { contentDescription = title }.padding(horizontal = 14.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Box(Modifier.width(20.dp), contentAlignment = Alignment.Center) { icon() }
        Text(title, style = Type.subheadline, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(detail, Modifier.weight(1f), style = Type.subheadline, color = if (warning) Palette.warning else Palette.secondary,
            maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** The `/` list: each command's icon, name and what it does; tapping one runs it (iOS `CommandList`). */
@Composable internal fun CommandList(commands: List<SlashCommand>, pick: (SlashCommand) -> Unit, modifier: Modifier = Modifier) {
    ComposerList(modifier) {
        for (command in commands) ComposerListRow(command.title, command.detail, { pick(command) }, command.warning) {
            Symbol(command.icon, size = 17.dp, tint = Palette.secondary)
        }
    }
}

/** The `@` list in a group: each member's face, name and description; tapping one mentions it (iOS `MentionList`). */
@Composable internal fun MentionList(members: List<Bot>, bots: List<Bot>, mention: (Bot) -> Unit, modifier: Modifier = Modifier) {
    ComposerList(modifier) {
        for (member in members) ComposerListRow(member.name, member.description, { mention(member) }) {
            BotAvatar(member, bots, 18.dp)
        }
    }
}
