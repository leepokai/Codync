package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
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
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.TextFieldValue
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

/** The pill and its menu: the three choices, then the sizes the menu lists and one the user types. */
@Composable private fun CompactionPicker(choice: Compaction, tokens: CompactionTokens?, select: (Compaction) -> Unit) {
    var typing by remember { mutableStateOf(false) }
    val current = if (choice is Compaction.Tokens) choice.label else tr(choice.label)
    DropdownMenu({ compactionRows(choice, tokens, select) { typing = true } }, label = "${tr("Summarize long chats")}: $current") {
        Row(Modifier.outline().pill(Color.Transparent), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(current, Modifier.weight(1f, fill = false), style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Symbol("chevron.down", size = 13.dp, tint = Palette.secondary)
        }
    }
    if (tokens != null) CustomSizeDialog(typing, (choice as? Compaction.Tokens)?.tokens, tokens, { typing = false }) { select(Compaction.Tokens(it)) }
}

private fun compactionRows(choice: Compaction, tokens: CompactionTokens?, select: (Compaction) -> Unit, custom: () -> Unit): List<MenuItem> {
    val presets = listOf(Compaction.Early, Compaction.NearLimit, Compaction.Off).map { preset ->
        MenuItem(preset.label, selected = choice == preset) { select(preset) }
    }
    if (tokens == null) return presets + MenuItem("Sizes show after the bot's next reply", divider = true, enabled = false)
    val picked = (choice as? Compaction.Tokens)?.tokens
    val sizes = tokens.presets.map { size ->
        MenuItem(Compaction.sizeLabel(size), selected = size == picked, verbatim = true) { select(Compaction.Tokens(size)) }
    }
    val own = MenuItem("Custom size…", selected = picked != null && picked !in tokens.presets, action = custom)
    return presets + MenuItem("Set a size", divider = true, children = sizes + own)
}

/** "Custom size": thousands of tokens, saved as the nearest size the window takes; Save waits for a number. */
@Composable private fun CustomSizeDialog(shown: Boolean, current: Long?, tokens: CompactionTokens, dismiss: () -> Unit, save: (Long) -> Unit) {
    var text by remember { mutableStateOf(TextFieldValue()) }
    // Set when it opens, with the cursor after the size; kept while it fades away.
    LaunchedEffect(shown) { if (shown) text = current?.let { (it / 1000).toString() }.orEmpty().let { TextFieldValue(it, TextRange(it.length)) } }
    val typed = Compaction.typed(text.text)
    val submit: () -> Unit = { typed?.let { save(tokens.fit(it)) } }
    CodyncDialog("Custom size", shown, dismiss, message = "Thousands of tokens, rounded to 10K, up to what the model holds.", field = {
        val focus = remember { FocusRequester() }
        LaunchedEffect(Unit) { focus.requestFocus() }
        PlainTextField(text, { text = it }, Modifier.fillMaxWidth().focusRequester(focus).pill(Palette.background),
            "${tokens.min / 1000}–${tokens.max / 1000}", singleLine = true,
            keyboard = KeyboardOptions(keyboardType = KeyboardType.Decimal, imeAction = ImeAction.Done),
            actions = KeyboardActions(onDone = { if (typed != null) { dismiss(); submit() } }))
    }) { listOf(DialogAction("Save", enabled = typed != null, action = submit)) }
}
