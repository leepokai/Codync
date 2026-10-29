package com.codync.android

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
import androidx.compose.ui.layout.onVisibilityChanged
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.MemoryDraft
import com.codync.android.core.MemoryFact
import com.codync.android.design.*
import java.text.DateFormat
import java.util.Date

private val FILTERS = listOf("all" to "All", "profile" to "About you", "pinned" to "Pinned", "review" to "Needs review")
private enum class Backup { Export, Import }

/** What a bot remembers, in its settings (iOS `MemoryCard`): search, edit, pin, history, backups. */
@Composable fun MemoryCard(botId: String, store: AppStore) {
    val scope = rememberCoroutineScope()
    val memory = remember(botId) { MemoryModel(botId, store, scope) }
    var editor by remember { mutableStateOf<MemoryDraft?>(null) }
    var history by remember { mutableStateOf(false) }
    var backup by remember { mutableStateOf<Backup?>(null) }
    var confirmClear by remember { mutableStateOf(false) }
    LaunchedEffect(botId) { memory.load() }
    // At the end of bot details: viewed once it scrolls into sight, once per opening.
    var viewed by remember(botId) { mutableStateOf(false) }
    val seen = Modifier.onVisibilityChanged(minFractionVisible = 0f) { visible ->
        if (visible && !viewed) { viewed = true; Analytics.capture(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Memory)) }
    }
    CardSection("Memory", modifier = seen, accessory = {
        IconButton("Add memory", "plus", enabled = !memory.busy) { editor = MemoryDraft.of(botId, null) }
        DropdownMenu({ listOf(
            MenuItem("Refresh", icon = "arrow.clockwise") { memory.refresh() },
            MenuItem("Export", icon = "square.and.arrow.up") { backup = Backup.Export; memory.export() },
            MenuItem("Import", icon = "square.and.arrow.down") { backup = Backup.Import },
            MenuItem("Forget everything", icon = "trash", destructive = true, divider = true) { confirmClear = true },
        ) }, Modifier.size(32.dp), label = "More") { Symbol("ellipsis", size = 20.dp, tint = Palette.secondary) }
    }) {
        row {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                SearchField(memory.query, { memory.search(it) }, Modifier.weight(1f), "Search memories")
                ChoicePicker(memory.filter, FILTERS, { memory.search(memory.query, it) }, fill = Palette.bubbleAgent, label = "Show")
            }
        }
        memory.error?.let { row { SelectionContainer { Text(it, style = Type.body, color = Palette.danger) } } }
        if (!memory.loaded && memory.error == null) row { Text("Loading memory…", style = Type.body, color = Palette.secondary) }
        if (memory.loaded && memory.facts.isEmpty()) row {
            Column(Modifier.fillMaxWidth().padding(vertical = 18.dp), horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Symbol("brain", size = 24.dp, tint = Palette.tertiary)
                Text(if (memory.query.isEmpty() && memory.filter == "all") "Nothing yet. Important details are remembered as you chat." else "No matching memories.",
                    style = Type.body, color = Palette.secondary, textAlign = TextAlign.Center)
            }
        }
        for (fact in memory.facts) row {
            FactRow(fact, memory.busy, { editor = MemoryDraft.of(botId, fact) }) { pick ->
                when (pick) {
                    "history" -> { history = true; memory.inspect(fact) }
                    else -> memory.action(pick, fact)
                }
            }
        }
        if (memory.nextOffset != null) row {
            Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                LinkButton("Load more (${memory.total})", color = Palette.secondary) { memory.refresh(more = true) }
            }
        }
    }
    CodyncSheet(editor, { editor = null }) { MemoryEditor(memory, it) }
    CodyncSheet(history, { history = false }) { MemoryHistory(memory) }
    CodyncSheet(backup, { backup = null }) { MemoryBackup(memory, it) }
    CodyncDialog("Forget everything this bot remembers?", confirmClear, { confirmClear = false }) {
        listOf(DialogAction("Forget everything", destructive = true) { memory.action("clearMemory") })
    }
}

@Composable private fun FactRow(fact: MemoryFact, busy: Boolean, open: () -> Unit, act: (String) -> Unit) {
    Row(verticalAlignment = Alignment.Top, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Column(Modifier.weight(1f).plainButton(action = open), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                if (fact.pinned) Symbol("pin.fill", size = 12.dp, tint = Palette.secondary)
                Text(fact.title.ifEmpty { fact.content.take(40) }, style = Type.body.semibold, color = Palette.text)
            }
            Text(fact.content, style = Type.body, color = Palette.text, maxLines = 3, overflow = TextOverflow.Ellipsis)
            Text("${if (fact.kind == "profile") "About you" else fact.memoryType} · ${DateFormat.getDateInstance(DateFormat.MEDIUM).format(Date(fact.createdAt))} · " +
                "${fact.revisionCount} revisions", style = Type.caption, color = Palette.secondary)
        }
        DropdownMenu({ buildList {
            add(MenuItem(if (fact.pinned) "Unpin" else "Pin", icon = if (fact.pinned) "pin.slash" else "pin") { act("pinMemory") })
            add(MenuItem("History", icon = "clock.arrow.circlepath") { act("history") })
            if (fact.reviewAfter != null) add(MenuItem("Mark reviewed", icon = "checkmark.circle") { act("reviewMemory") })
            add(MenuItem("Forget", icon = "trash", destructive = true, divider = true) { act("forgetMemory") })
        } }, Modifier.size(32.dp), enabled = !busy, label = "Memory actions") { Symbol("ellipsis", size = 20.dp, tint = Palette.secondary) }
    }
}

/** Add or edit one memory (iOS `MemoryEditorView`). */
@Composable private fun MemoryEditor(memory: MemoryModel, initial: MemoryDraft) {
    val dismiss = LocalDismissModal.current
    var draft by remember(initial) { mutableStateOf(initial) }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(if (draft.id == null) "Add memory" else "Edit memory")
        Column(Modifier.verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            LabeledBox("Title") { FieldBox(draft.title, { draft = draft.copy(title = it) }, "Title", fill = Palette.surface) }
            LabeledBox("Memory") {
                Box(Modifier.heightIn(min = 160.dp)) { FieldBox(draft.content, { draft = draft.copy(content = it) }, "", 7, 20, fill = Palette.surface) }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                for ((scope, label) in listOf("project" to "Work", "personal" to "About you", "global" to "General")) {
                    LinkButton(label, style = if (draft.scope == scope) Type.body.semibold else Type.body) { draft = draft.copy(scope = scope) }
                }
            }
            LabeledBox("Category") { FieldBox(draft.memoryType, { draft = draft.copy(memoryType = it) }, "Category", fill = Palette.surface, capitalize = false) }
            LabeledBox("Topic (optional)") { FieldBox(draft.topicKey, { draft = draft.copy(topicKey = it) }, "Topic (optional)", fill = Palette.surface, capitalize = false) }
            memory.error?.let { Text(it, style = Type.body, color = Palette.danger) }
            PrimaryButton(if (memory.busy) "Saving…" else "Save", enabled = !memory.busy && draft.title.isNotBlank() && draft.content.isNotBlank()) {
                memory.save(draft) { dismiss() }
            }
        }
    }
}

@Composable private fun LabeledBox(title: String, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(title, style = Type.caption, color = Palette.secondary)
        content()
    }
}

/** A memory's earlier versions and the session it came from (iOS `MemoryHistoryView`). */
@Composable private fun MemoryHistory(memory: MemoryModel) {
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("Memory history")
        Column(Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            memory.error?.let { Text(it, style = Type.body, color = Palette.danger) }
            val detail = memory.detail
            if (detail == null) Text("Loading history…", style = Type.body, color = Palette.secondary)
            else {
                SelectionContainer { Text(detail.history.result, style = Type.body, color = Palette.text) }
                detail.history.cursor?.let { cursor -> LinkButton("Older versions") { memory.inspect(detail.fact, cursor) } }
                Text("Session timeline", style = Type.body.semibold, color = Palette.text)
                SelectionContainer { Text(detail.timeline.result, style = Type.body, color = Palette.text) }
            }
        }
    }
}

/** Copy an Engram backup out, or paste one in (iOS `MemoryBackupView`). */
@Composable private fun MemoryBackup(memory: MemoryModel, mode: Backup) {
    val dismiss = LocalDismissModal.current
    var text by remember { mutableStateOf("") }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(if (mode == Backup.Export) "Export memories" else "Import memories")
        Column(Modifier.verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Text(if (mode == Backup.Export) "Copy this Engram backup to keep or import your memories."
                else "Paste an Engram JSON backup. Import merges memories and preserves newer changes. A backup is saved first.",
                style = Type.body, color = Palette.text)
            if (mode == Backup.Export) SelectionContainer {
                Text(memory.backup.ifEmpty { "Preparing backup…" }, style = Type.caption.monospaced, color = Palette.text)
            } else {
                Box(Modifier.heightIn(min = 280.dp)) { FieldBox(text, { text = it }, "", 12, 40, style = Type.caption.monospaced, fill = Palette.surface, capitalize = false) }
                PrimaryButton("Import", enabled = !memory.busy && text.isNotEmpty()) { memory.importBackup(text) { dismiss() } }
            }
            memory.error?.let { Text(it, style = Type.body, color = Palette.danger) }
        }
    }
}
