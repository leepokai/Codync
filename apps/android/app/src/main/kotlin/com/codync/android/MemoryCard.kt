package com.codync.android

import android.content.res.Configuration
import android.text.format.DateUtils
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.selection.SelectionContainer
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
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.BotMemory
import com.codync.android.core.DreamRun
import com.codync.android.core.MemoryEntry
import com.codync.android.core.MemoryFileDetail
import com.codync.android.core.MemoryFiles
import com.codync.android.design.*
import kotlinx.coroutines.launch

private enum class MemorySection { Yours, Bot, Dreams, Harness }

/** What an info button explains. */
private enum class MemoryInfo { Harness, Dreams }

/** A note the sheet opens: its name and whose folder holds it. */
private data class MemoryNote(val name: String, val shared: Boolean)

/**
 * A bot's memory, read-only (iOS `MemoryCard`): what all your bots know about you, what this one knows, its dreams,
 * and its harness's own notes when the user kept them on. The user changes memory by telling the bot.
 */
@Composable internal fun MemoryCard(botId: String, state: AppState, store: AppStore) {
    val bot = state.bots.firstOrNull { it.id == botId }
    val memory = remember(botId) { MemoryModel(botId, store) }
    val scope = rememberCoroutineScope()
    var section by remember(botId) { mutableStateOf(MemorySection.Bot) }
    var info by remember { mutableStateOf<MemoryInfo?>(null) }
    var note by remember { mutableStateOf<MemoryNote?>(null) }
    // Entries and dreams showing all their lines.
    var unfolded by remember(botId) { mutableStateOf(emptySet<String>()) }
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Memory))
    // Memory changes anywhere move the bot's `memoryRev`: fetch again.
    LaunchedEffect(botId, bot?.memoryRev, bot?.sharedMemory, bot?.harnessMemory) { memory.load() }
    val view = memory.memory
    // The harness's tab goes when its memory is turned off (on another device too).
    LaunchedEffect(view?.projectMemory == null) { if (view != null && view.projectMemory == null && section == MemorySection.Harness) section = MemorySection.Bot }
    // A long memory stops at a few lines, fewer when the screen is short (landscape); tapping it shows the rest.
    val short = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    val lines = { key: String -> if (key in unfolded) Int.MAX_VALUE else if (short) 2 else 5 }
    val unfold = { key: String -> unfolded = if (key in unfolded) unfolded - key else unfolded + key }
    val harness = backendName(state, bot?.backend ?: "claude")
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        TabChoice(section, listOfNotNull(MemorySection.Yours to "About you", MemorySection.Bot to "This bot", MemorySection.Dreams to "Dreams",
            (MemorySection.Harness to harness).takeIf { view?.projectMemory != null }), { picked ->
            section = picked
            // Claude Code writes its notes outside Codync, so nothing moves `memoryRev`: read them again on the way in.
            if (picked == MemorySection.Harness) scope.launch { memory.load() }
        })
        CardSection {
            memory.error?.let { error -> row { SelectionContainer { Text(tr(error), color = Palette.danger) } } }
            when {
                view == null -> if (memory.error == null) row { Text(tr("Loading memory…"), color = Palette.secondary) }
                section == MemorySection.Yours || section == MemorySection.Bot ->
                    layerRows(view, bot, section == MemorySection.Yours, state, store, harness, lines, unfold, { info = MemoryInfo.Harness }) { note = it }
                section == MemorySection.Dreams -> dreamRows(view.dreams, lines, unfold) { info = MemoryInfo.Dreams }
                else -> harnessRows(view.projectMemory)
            }
        }
    }
    CodyncSheet(info, { info = null }) { shown ->
        when (shown) {
            MemoryInfo.Harness -> InfoSheet(harnessInfo(harness, when {
                bot?.harnessMemoryShared == true -> HarnessAccess.Shared
                bot?.backend == "claude" -> HarnessAccess.Readable
                else -> HarnessAccess.Hidden
            }))
            MemoryInfo.Dreams -> InfoSheet(DREAMS_INFO)
        }
    }
    CodyncSheet(note, { note = null }) { MemoryNoteView(memory, it) }
}

/** One layer: its switches, what it holds against its cap, its entries and its notes. */
private fun CardRows.layerRows(view: BotMemory, bot: Bot?, shared: Boolean, state: AppState, store: AppStore, harness: String,
    lines: (String) -> Int, unfold: (String) -> Unit, aboutHarness: () -> Unit, open: (MemoryNote) -> Unit) {
    val layer = if (shared) view.yours else view.bot
    if (shared && bot != null) row {
        CodyncSwitch(bot.sharedMemory ?: view.shared, { store.setSharedMemory(bot, it) }, Modifier.fillMaxWidth()) {
            Text(tr("Use what your bots share"), style = Type.body, color = Palette.text)
        }
    }
    // The harness's own memory: a switch where Codync can turn it off, "Shared" where every bot on it shares it.
    if (!shared && bot != null && (bot.harnessMemory != null || bot.harnessMemoryShared == true)) row {
        HarnessMemoryRow(bot, harness, store, aboutHarness)
    }
    row {
        Text(tr(if (shared) "Your name, time zone, people and lasting preferences, shared by your bots. {used} of {cap} in use. To change it, tell the bot."
            else "What this bot learned about you. {used} of {cap} in use. To change it, tell the bot.")
            .replace("{used}", "${layer.used}").replace("{cap}", "${layer.cap}"), style = Type.footnote, color = Palette.secondary)
    }
    if (layer.entries.isEmpty()) row { EmptyMemory("brain", if (shared) "Nothing shared yet." else "Nothing yet. What this bot learns about you shows up here.") }
    for (entry in layer.entries) row { EntryRow(entry, state, lines("entry:${entry.id}")) { unfold("entry:${entry.id}") } }
    view.files?.let { files -> noteRows(files, shared, open) }
}

@Composable private fun HarnessMemoryRow(bot: Bot, harness: String, store: AppStore, about: () -> Unit) {
    val own = tr("{name}'s own memory").replace("{name}", harness)
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(own, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        IconButton(tr("About {name}'s own memory").replace("{name}", harness), "info.circle", action = about)
        Spacer(Modifier.weight(1f))
        val on = bot.harnessMemory
        if (on != null) CodyncSwitch(on, { store.setHarnessMemory(bot, it) }, Modifier.width(34.dp)) {}
        else Text(tr("Shared"), style = Type.body, color = Palette.secondary, maxLines = 1)
    }
}

@Composable private fun EntryRow(entry: MemoryEntry, state: AppState, lines: Int, unfold: () -> Unit) {
    Column(Modifier.fillMaxWidth().plainButton(action = unfold), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            if (entry.pinned) Symbol("pin.fill", size = 12.dp, tint = Palette.secondary)
            Text(entry.text, style = Type.body, color = if (entry.archived) Palette.tertiary else Palette.text, maxLines = lines, overflow = TextOverflow.Ellipsis)
        }
        val author = when (entry.author) {
            "user" -> tr("You wrote it")
            "bot" -> tr("{name} remembered it").replace("{name}", state.bots.firstOrNull { it.id == entry.authorBot }?.name ?: tr("A bot"))
            "dream" -> tr("Dreaming")
            else -> tr("Moved from the old memory")
        }
        Text("$author · ${relative(entry.updatedAt)}${if (entry.archived) " · " + tr("archived") else ""}", style = Type.caption, color = Palette.secondary)
    }
}

/** One layer's long-form notes under its entries: name, description, size and date, marked past a cap. The bot writes them. */
private fun CardRows.noteRows(files: MemoryFiles, shared: Boolean, open: (MemoryNote) -> Unit) {
    val live = (if (shared) files.yours else files.bot).files.filter { !it.deleted }
    row { Text(tr("Notes"), style = Type.footnote.semibold, color = Palette.secondary) }
    if (live.isEmpty()) row { Text(tr("Longer notes, one topic each. The bot reads one when its topic comes up."), style = Type.footnote, color = Palette.tertiary) }
    else if (live.size > files.folderCap) row {
        Text(tr("{count} notes, over the {cap}-note cap. Ask the bot to merge some.").replace("{count}", "${live.size}").replace("{cap}", "${files.folderCap}"),
            style = Type.footnote, color = Palette.attention)
    }
    for (file in live) row {
        Column(Modifier.fillMaxWidth().plainButton { open(MemoryNote(file.name, shared)) }, verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Symbol("doc.text", size = 18.dp, tint = Palette.text)
                Text(file.name, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            file.description?.let { Text(it, style = Type.footnote, color = Palette.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis) }
            val over = file.tokens > files.cap
            Text(tr(if (over) "{tokens} tokens, over the {cap} cap" else "{tokens} tokens").replace("{tokens}", "${file.tokens}")
                .replace("{cap}", "${files.cap}") + " · " + relative(file.updatedAt), style = Type.caption, color = if (over) Palette.attention else Palette.secondary)
        }
    }
}

/** What dreaming did. */
private fun CardRows.dreamRows(runs: List<DreamRun>, lines: (String) -> Int, unfold: (String) -> Unit, about: () -> Unit) {
    row {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(tr("Between chats the bot tidies its memory from your own words."), Modifier.weight(1f), style = Type.footnote, color = Palette.secondary)
            IconButton("About dreaming", "info.circle", action = about)
        }
    }
    if (runs.isEmpty()) row { EmptyMemory("moon", "No dreams yet.") }
    for (run in runs) row {
        val limit = lines("dream:${run.id}")
        Column(Modifier.fillMaxWidth().plainButton { unfold("dream:${run.id}") }, verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(relative(run.at) + if (run.undone) " · " + tr("undone") else "", style = Type.body.semibold, color = Palette.text)
            for (change in run.added) Text("+ ${change.text}", style = Type.body, color = Palette.added, maxLines = limit, overflow = TextOverflow.Ellipsis)
            for (change in run.archived) Text("− ${change.text}", style = Type.body, color = Palette.removed, maxLines = limit, overflow = TextOverflow.Ellipsis)
            for (file in run.files.orEmpty()) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Symbol("doc.text", size = 16.dp, tint = Palette.secondary)
                Text(tr("Added to {name}").replace("{name}", file.name), style = Type.body, color = Palette.secondary)
            }
        }
    }
}

/** Claude Code's own notes, read-only here. */
private fun CardRows.harnessRows(project: BotMemory.Project?) {
    project ?: return
    row {
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(tr("Claude Code's own notes, read-only here."), style = Type.footnote, color = Palette.secondary)
            SelectionContainer { Text(project.path, style = Type.caption, color = Palette.tertiary) }
        }
    }
    if (project.text.isEmpty()) row { EmptyMemory("brain", "Nothing yet. Claude Code's notes show up here once it saves some.") }
    else row { SelectionContainer { Text(project.text, style = Type.body, color = Palette.text) } }
}

@Composable private fun EmptyMemory(icon: String, text: String) {
    Column(Modifier.fillMaxWidth().padding(vertical = 18.dp), horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Symbol(icon, size = 24.dp, tint = Palette.tertiary)
        Text(tr(text), style = Type.body, color = Palette.secondary, textAlign = TextAlign.Center)
    }
}

/** One note, to read (iOS `MemoryFileView`). */
@Composable private fun MemoryNoteView(memory: MemoryModel, note: MemoryNote) {
    var detail by remember(note) { mutableStateOf<MemoryFileDetail?>(null) }
    LaunchedEffect(note) { detail = memory.file(note.name, note.shared) }
    Column {
        ModalHeader(note.name)
        CardForm {
            memory.error?.let { error -> CardSection { row { SelectionContainer { Text(tr(error), color = Palette.danger) } } } }
            val shown = detail
            if (shown != null) CardSection(footer = shown.path) {
                row { SelectionContainer { Text(shown.content ?: tr("Deleted."), style = Type.footnote.copy(fontFamily = FontFamily.Monospace), color = Palette.text) } }
            } else CardSection { row { Text(tr("Loading…"), color = Palette.secondary) } }
        }
    }
}

/** "5 minutes ago", "Yesterday": the system's relative date, as iOS's `.relative(presentation: .named)`. */
private fun relative(milliseconds: Long): String =
    DateUtils.getRelativeTimeSpanString(milliseconds, System.currentTimeMillis(), DateUtils.MINUTE_IN_MILLIS).toString()
