package com.codync.android

import android.text.format.DateUtils
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import com.codync.android.core.BotMemory
import com.codync.android.core.DreamRun
import com.codync.android.core.MemoryEditTarget
import com.codync.android.core.MemoryEntry
import com.codync.android.design.*
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

private enum class Tab { Yours, Bot, Dreams, Harness }

/**
 * A bot's memory (iOS `MemoryCard`): what all your bots know about you, what this one knows, its dreams, and its
 * harness's own notes when the user kept them on.
 */
@Composable fun MemoryCard(botId: String, state: AppState, store: AppStore) {
    val scope = rememberCoroutineScope()
    val memory = remember(botId) { MemoryModel(botId, store, scope) }
    val bot = state.bots.firstOrNull { it.id == botId }
    val harness = bot?.let { backendName(state, it.backend) } ?: "Claude Code"
    val view = memory.view
    var tab by remember { mutableStateOf(Tab.Bot) }
    var editor by remember { mutableStateOf<MemoryEditTarget?>(null) }
    var confirmClear by remember { mutableStateOf(false) }
    var aboutHarness by remember { mutableStateOf(false) }
    // Memory changes anywhere move the bot's `memoryRev`: fetch again.
    LaunchedEffect(botId, bot?.memoryRev, bot?.sharedMemory, bot?.harnessMemory) { memory.load() }
    // The harness's section goes when its memory is turned off (on another device too).
    LaunchedEffect(view != null && view.projectMemory == null) { if (view != null && view.projectMemory == null && tab == Tab.Harness) tab = Tab.Bot }
    val shared = tab == Tab.Yours
    CardSection("Memory", accessory = {
        // Read-only: the user changes memory by telling the bot (host 2.21+, as on the other apps).
        IconButton("Refresh", "arrow.clockwise", enabled = !memory.busy) { memory.refresh() }
    }) {
        row {
            val options = listOf(Tab.Yours to "About you", Tab.Bot to "This bot", Tab.Dreams to "Dreams") +
                (if (view?.projectMemory != null) listOf(Tab.Harness to harness) else emptyList())
            SegmentedChoice(tab, options, { tab = it; if (it == Tab.Harness) memory.refresh() })
        }
        memory.error?.let { row { SelectionContainer { Text(it, style = Type.body, color = Palette.danger) } } }
        if (view == null) {
            if (memory.error == null) row { Text("Loading memory…", style = Type.body, color = Palette.secondary) }
        } else when (tab) {
            Tab.Yours, Tab.Bot -> layer(memory, view, bot, state, store, shared, harness, { editor = it }, { aboutHarness = true })
            Tab.Dreams -> dreams(memory, view.dreams)
            Tab.Harness -> harnessNotes(view.projectMemory, harness)
        }
    }
    CodyncSheet(editor, { editor = null }) { MemoryEditor(memory, it) }
    CodyncSheet(aboutHarness, { aboutHarness = false }) { HarnessMemorySheet(harness, bot?.backend == "claude") }
    CodyncDialog(if (shared) "Forget everything your bots share about you?" else "Forget everything this bot remembers?", confirmClear,
        { confirmClear = false }) {
        listOf(DialogAction("Forget everything", destructive = true) { memory.mutate("clearMemory", shared = shared) })
    }
}

private fun CardRows.layer(memory: MemoryModel, view: BotMemory, bot: com.codync.android.core.Bot?, state: AppState, store: AppStore,
    shared: Boolean, harness: String, edit: (MemoryEditTarget) -> Unit, about: () -> Unit) {
    val layer = if (shared) view.yours else view.bot
    if (shared && bot != null) row {
        SwitchRow("Use what your bots share", null, bot.sharedMemory ?: view.shared, {
            store.command("updateBot", buildJsonObject { put("id", bot.id); put("sharedMemory", it) }, "memory:" + bot.id)
        })
    }
    val harnessOn = bot?.harnessMemory
    if (!shared && bot != null && harnessOn != null) row {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("$harness's own memory", Modifier.weight(1f), style = Type.body, color = Palette.text, maxLines = 1)
            IconButton("About $harness's own memory", "info.circle", action = about)
            CodyncSwitch(harnessOn, {
                store.command("updateBot", buildJsonObject { put("id", bot.id); put("harnessMemory", it) }, "memory:" + bot.id)
            }, Modifier.width(34.dp), !memory.busy) {}
        }
    }
    row {
        Text((if (shared) "Your name, time zone, people and lasting preferences, shared by your bots." else "What this bot learned about you.") +
            " ${layer.used} of ${layer.cap} in use (${when {
                layer.used.toFloat() / maxOf(1, layer.cap) >= 0.9f -> "🔴 Near capacity — auto-rotates"
                layer.used.toFloat() / maxOf(1, layer.cap) >= 0.7f -> "🟡 Moderate usage"
                else -> "🟢 Healthy budget"
            }}).", style = Type.footnote, color = Palette.secondary)
    }
    if (layer.entries.isEmpty()) row {
        MemoryEmpty("brain", if (shared) "Nothing shared yet." else "Nothing yet. What this bot learns about you shows up here, and you can change any of it.")
    }
    for (entry in layer.entries) row { EntryRow(memory, entry, shared, state) { edit(MemoryEditTarget(entry.id, shared, entry.text)) } }
    view.files?.let { files -> row { MemoryNotes(memory, files, shared, state) } }
}

@Composable private fun EntryRow(memory: MemoryModel, entry: MemoryEntry, shared: Boolean, state: AppState, open: () -> Unit) {
    Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                if (entry.pinned) Symbol("pin.fill", size = 12.dp, tint = Palette.secondary, modifier = Modifier.padding(top = 4.dp))
                Text(entry.text, style = Type.body, color = if (entry.archived) Palette.tertiary else Palette.text)
            }
            Text("${author(entry, state)} · ${ago(entry.updatedAt)}${if (entry.archived) " · archived" else ""}", style = Type.caption, color = Palette.secondary)
        }
    }
}

private fun CardRows.dreams(memory: MemoryModel, runs: List<DreamRun>) {
    row { Text("Between chats the bot tidies its memory from your own words. Each run can be undone.", style = Type.footnote, color = Palette.secondary) }
    if (runs.isEmpty()) row { MemoryEmpty("moon.zzz.fill", "No dreams yet.") }
    for (run in runs) row {
        Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                Text(ago(run.at) + if (run.undone) " · undone" else "", style = Type.body.semibold, color = Palette.text)
                for (added in run.added) Text("+ ${added.text}", style = Type.body, color = Palette.text)
                for (gone in run.archived) Text("− ${gone.text}", style = Type.body, color = Palette.tertiary)
                for (note in run.files.orEmpty()) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Symbol("doc.text", size = 14.dp, tint = Palette.secondary)
                    Text("Added to ${note.name}", style = Type.body, color = Palette.secondary)
                }
            }
        }
    }
}

private fun CardRows.harnessNotes(project: BotMemory.Project?, name: String) {
    if (project == null) return
    row { Text("$name's own notes, read-only here.", style = Type.footnote, color = Palette.secondary) }
    row { SelectionContainer { Text(project.path, style = Type.caption, color = Palette.tertiary) } }
    if (project.text.isEmpty()) row { MemoryEmpty("brain", "Nothing yet. $name's notes show up here once it saves some.") }
    else row { SelectionContainer { Text(project.text, style = Type.body, color = Palette.text, modifier = Modifier.fillMaxWidth()) } }
}

@Composable private fun MemoryEmpty(icon: String, text: String) {
    Column(Modifier.fillMaxWidth().padding(vertical = 18.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Symbol(icon, size = 24.dp, tint = Palette.tertiary)
        Text(text, style = Type.body, color = Palette.secondary, textAlign = TextAlign.Center)
    }
}

private fun author(entry: MemoryEntry, state: AppState): String = when (entry.author) {
    "user" -> "You wrote it"
    "bot" -> "${state.bots.firstOrNull { it.id == entry.authorBot }?.name ?: "A bot"} remembered it"
    "dream" -> "Dreaming"
    else -> "Moved from the old memory"
}

internal fun ago(millis: Long): String =
    DateUtils.getRelativeTimeSpanString(minOf(millis, System.currentTimeMillis()), System.currentTimeMillis(), DateUtils.MINUTE_IN_MILLIS).toString()

/** Writes one memory: a new one, or a change to an entry (iOS `MemoryEditorView`). */
@Composable private fun MemoryEditor(memory: MemoryModel, target: MemoryEditTarget) {
    val dismiss = LocalDismissModal.current
    var text by remember(target) { mutableStateOf(target.text) }
    LaunchedEffect(target) { memory.error = null }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(if (target.entry != null) "Edit memory" else if (target.shared) "Add to what your bots share" else "Add memory")
        Column(Modifier.verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Column(Modifier.heightIn(min = 96.dp)) { FieldBox(text, { text = it }, "One short fact", 2, 6, fill = Palette.surface) }
            memory.error?.let { Text(it, style = Type.body, color = Palette.danger) }
            PrimaryButton(if (memory.busy) "Saving…" else "Save", Modifier.fillMaxWidth(), enabled = !memory.busy && text.isNotBlank()) {
                memory.mutate("saveMemory", id = target.entry, shared = target.shared, text = text.trim(), done = { dismiss() })
            }
        }
    }
}
