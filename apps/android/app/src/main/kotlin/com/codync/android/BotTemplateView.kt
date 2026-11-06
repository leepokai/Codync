package com.codync.android

import android.content.ClipData
import android.content.ClipboardManager
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import com.codync.android.core.BotDraft
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle

/** A portable settings snapshot (iOS `BotTemplateView`): the bot's setup to copy, without its conversation or this computer's fallbacks. */
@Composable internal fun BotTemplateView(draft: BotDraft) {
    val context = LocalContext.current
    val template = remember(draft) { draft.template() }
    var copied by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("Create template")
        Column(Modifier.weight(1f).padding(start = 24.dp, end = 24.dp, bottom = 24.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
            Text(draft.name, style = Type.subheadline, color = Palette.secondary)
            Text(tr("Copy this bot’s settings to reuse as a template. Conversation history and conversation ID are excluded."),
                style = Type.callout, color = Palette.secondary)
            SelectionContainer(Modifier.weight(1f).fillMaxWidth().clip(RoundedRectangle(12.dp)).background(Palette.surface)
                .verticalScroll(rememberScrollState()).padding(16.dp)) {
                Text(template, style = Type.system(12f).monospaced, color = Palette.text)
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Text(tr("Includes workspace preferences and agent configuration."), Modifier.weight(1f), style = Type.caption, color = Palette.secondary)
                PrimaryButton(if (copied) "Copied" else "Copy template") {
                    context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText(draft.name, template))
                    copied = true
                }
            }
        }
    }
}
