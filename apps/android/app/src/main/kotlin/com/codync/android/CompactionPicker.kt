package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.BotDraft
import com.codync.android.core.Compaction
import com.codync.android.core.CompactionTokens
import com.codync.android.design.*

/** "Summarize long chats": only where Codync summarizes the bot's chat (the host sends a choice). */
internal fun CardRows.compactionRow(bot: Bot?, draft: BotDraft, change: (BotDraft) -> Unit) {
    val choice = draft.compaction ?: return
    row {
        ValueRow("Summarize long chats", detail = "When Codync summarizes this chat so it can go on. If the chat fills up first, the agent summarizes it itself.") {
            CompactionPicker(choice, bot?.compactionTokens) { change(draft.copy(compaction = it)) }
        }
    }
}

/** The pill and its menu: the three choices, then the sizes the agent's window allows (a scrolling submenu). */
@Composable private fun CompactionPicker(choice: Compaction, tokens: CompactionTokens?, select: (Compaction) -> Unit) {
    val current = if (choice is Compaction.Tokens) choice.label else tr(choice.label)
    DropdownMenu({ compactionRows(choice, tokens, select) }, label = "${tr("Summarize long chats")}: $current") {
        Row(Modifier.outline().pill(Color.Transparent), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(current, Modifier.weight(1f, fill = false), style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Symbol("chevron.down", size = 13.dp, tint = Palette.secondary)
        }
    }
}

private fun compactionRows(choice: Compaction, tokens: CompactionTokens?, select: (Compaction) -> Unit): List<MenuItem> {
    val presets = listOf(Compaction.Early, Compaction.NearLimit, Compaction.Off).map { preset ->
        MenuItem(preset.label, selected = choice == preset) { select(preset) }
    }
    val picked = (choice as? Compaction.Tokens)?.tokens
    val sizes = if (tokens == null) MenuItem("Sizes show after the bot's next reply", divider = true, enabled = false)
    else MenuItem("Set a size", divider = true, children = tokens.sizes.map { size ->
        MenuItem(Compaction.sizeLabel(size), selected = size == picked, verbatim = true) { select(Compaction.Tokens(size)) }
    })
    return presets + sizes
}
