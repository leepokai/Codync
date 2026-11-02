package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import com.codync.android.core.Entry
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject

/**
 * The bot's chat starts over (iOS `HandoffCardRow`): what the new conversation carries, which the user can edit until
 * their next message sends it, or turn down to carry on with this one. Once sent, the line unfolds to what it carried.
 */
@Composable internal fun HandoffCardRow(entry: Entry, card: JsonObject, state: AppState, store: AppStore) {
    var expanded by remember { mutableStateOf(false) }
    var editing by remember { mutableStateOf(false) }
    val carried = card.text("text").orEmpty()
    val label: @Composable RowScope.() -> Unit = {
        Symbol("arrow.triangle.branch", size = 13.dp, tint = Palette.secondary)
        Spacer(Modifier.width(6.dp))
        Text(entry.text, Modifier.weight(1f, fill = false), style = Type.footnote, color = Palette.secondary)
    }
    val what: @Composable () -> Unit = {
        SelectionContainer {
            Text(carried, Modifier.fillMaxWidth(), style = Type.footnote.copy(fontFamily = FontFamily.Monospace), color = Palette.secondary)
        }
    }
    if (card.text("status") == "draft") {
        Column(Modifier.padding(end = 40.dp).clip(RoundedRectangle(14.dp)).background(Palette.bubbleAgent).padding(12.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, content = label)
            Disclosure(expanded, { expanded = !expanded }, Modifier.fillMaxWidth(), {
                Text(tr("What it carries"), style = Type.footnote.medium, color = Palette.text)
            }) { what() }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if ("handoff:${entry.id}" in state.busy) Spinner()
                else {
                    SecondaryButton("Keep this conversation") { store.keepConversation(entry) }
                    IconButton("Edit what it carries", "pencil") { editing = true }
                }
            }
        }
        CodyncSheet(editing, { editing = false }) { HandoffEditor(entry, carried, store) }
    } else Disclosure(expanded, { expanded = !expanded }, Modifier.fillMaxWidth(), label) { what() }
}

/** Edits what the next conversation starts with (iOS `HandoffEditorView`). */
@Composable private fun HandoffEditor(entry: Entry, initial: String, store: AppStore) {
    var text by remember { mutableStateOf(initial) }
    var saving by remember { mutableStateOf(false) }
    val dismiss = LocalDismissModal.current
    val scope = rememberCoroutineScope()
    val trimmed = text.trim()
    Column {
        ModalHeader("Edit the handoff") {
            if (saving) Spinner()
            else IconButton("Save", "checkmark", enabled = trimmed.isNotEmpty()) {
                saving = true
                scope.launch { if (store.editHandoff(entry, trimmed)) dismiss(); saving = false }
            }
        }
        CardForm {
            CardSection(footer = "Your next message starts a fresh conversation that begins with this.") {
                row {
                    val style = Type.footnote.copy(fontFamily = FontFamily.Monospace, color = Palette.text)
                    BasicTextField(text, { text = it }, Modifier.fillMaxWidth().heightIn(min = 180.dp), textStyle = style,
                        cursorBrush = SolidColor(Palette.accent), decorationBox = { field ->
                            if (text.isEmpty()) Text(tr("What the new conversation should know"), style = style, color = Palette.tertiary)
                            field()
                        })
                }
            }
        }
    }
}
