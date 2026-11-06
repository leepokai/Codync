package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.Entry
import com.codync.android.core.TraceFilter
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonPrimitive

/** "Full conversation": everything the agent did, grouped by turn, with typed filters (iOS `TraceView`). */
@Composable internal fun TraceView(bot: Bot, thread: String?, state: AppState) {
    val all = state.entries.filter { it.botId == bot.id && it.threadId == thread }
    val counts = remember(all) { TraceFilter.counts(all) }
    var filter by remember { mutableStateOf(TraceFilter.All) }
    val turns = remember(all, filter) { all.filter { filter.matches(it) }.groupBy { it.turn }.toSortedMap().toList() }
    val list = rememberLazyListState()
    // Where the reader was before the last filter change, to stay put across it.
    val anchor = remember { mutableStateOf<Long?>(null) }
    val beforeTurns = remember { mutableStateOf<List<Long>>(emptyList()) }
    LaunchedEffect(Unit) { if (turns.isNotEmpty()) list.scrollToItem(turns.lastIndex, Int.MAX_VALUE) }
    LaunchedEffect(filter) {
        val target = TraceFilter.anchorIndex(beforeTurns.value, anchor.value, turns.map { it.first })
        if (target != null) list.scrollToItem(target)
        anchor.value = null
    }
    Column(Modifier.fillMaxSize()) {
        ModalHeader("Full conversation")
        TraceFilters(filter, counts) { next ->
            anchor.value = list.layoutInfo.visibleItemsInfo.firstOrNull()?.key as? Long
            beforeTurns.value = turns.map { it.first }
            filter = next
        }
        LazyColumn(state = list, contentPadding = PaddingValues(start = 20.dp, end = 20.dp, bottom = 20.dp), verticalArrangement = Arrangement.spacedBy(22.dp)) {
            if (turns.isEmpty()) item { Text("Nothing yet.", style = Type.body, color = Palette.tertiary) }
            items(turns, key = { it.first }) { (turn, entries) ->
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(entries.firstOrNull { it.kind == "user" }?.text ?: "Turn $turn", Modifier.padding(start = 4.dp),
                        style = Type.subheadline.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    CardSection { for (entry in entries) row { TraceRow(entry) } }
                }
            }
        }
    }
}

/** The "Full conversation" filters: one pill per event type with its count (idea 32). */
@Composable private fun TraceFilters(selection: TraceFilter, counts: Map<TraceFilter, Int>, select: (TraceFilter) -> Unit) {
    Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 20.dp).padding(bottom = 12.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        for (filter in TraceFilter.entries) {
            val count = counts[filter] ?: 0
            val label = if (filter == TraceFilter.All) filter.label else "${filter.label} $count"
            TraceChip(label, filter == selection) { select(filter) }
        }
    }
}

@Composable private fun TraceChip(label: String, on: Boolean, action: () -> Unit) {
    val fill = if (on) Palette.accentFill else Palette.surface
    val ink = if (on) Palette.onAccent else Palette.secondary
    Row(Modifier.height(32.dp).clip(Capsule()).background(fill)
        .clickable(role = Role.Tab, onClick = action)
        .semantics { contentDescription = label; selected = on }
        .padding(horizontal = 12.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(label, style = Type.caption.medium, color = ink, maxLines = 1)
    }
}

/** What the running turn has done so far, in order: the steps the working indicator unfolds. */
internal fun workingSteps(entries: List<Entry>): List<Entry> {
    val turn = runningTurn(entries, working = true)
    return entries.filter { it.turn == turn && it.isWorkingStep }
}

/** The running turn's thoughts start `unfolded`, so its reasoning reads as it comes in. */
@Composable internal fun TraceRow(entry: Entry, unfolded: Boolean = false) {
    val d = entry.data
    var expanded by remember(entry.id) { mutableStateOf(unfolded) }
    when (entry.kind) {
        "user" -> Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Symbol("person.fill", size = 16.dp, tint = Palette.secondary)
            Text(entry.text, style = Type.subheadline, color = Palette.text)
        }
        "agent" -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(if (d["final"]?.jsonPrimitive?.booleanOrNull == true) "Reply" else "Said", style = Type.caption2.bold, color = Palette.tertiary)
            MarkdownText(entry.text)
        }
        "thought" -> Disclosure(expanded, { expanded = !expanded }, label = {
            ThinkingOrb(size = 16.dp, color = Palette.secondary, animated = false)
            Text("Thinking", Modifier.padding(start = 8.dp), style = Type.subheadline, color = Palette.secondary)
        }) { SelectionContainer { Text(entry.text, style = Type.footnote, color = Palette.secondary) } }
        "tool" -> ToolRow(d)
        "plan" -> Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Symbol("checklist", size = 16.dp, tint = Palette.text)
                Text("Plan", style = Type.subheadline.bold, color = Palette.text)
            }
            for (item in (d["entries"] as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }) {
                val status = item.text("status")
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Symbol(when (status) { "completed" -> "checkmark.circle.fill"; "in_progress" -> "circle.dotted"; else -> "circle" }, size = 15.dp,
                        tint = if (status == "completed") Palette.accent else Palette.tertiary)
                    Text(item.text("content").orEmpty(), style = Type.footnote.copy(textDecoration = if (status == "completed") TextDecoration.LineThrough else null),
                        color = Palette.text)
                }
            }
        }
        "permission" -> Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Symbol("hand.raised", size = 16.dp, tint = Palette.secondary)
            Text(d.text("title") ?: "Approval", style = Type.subheadline, color = Palette.secondary)
        }
        else -> Text(entry.text, style = Type.footnote, color = Palette.tertiary)
    }
}

/** A tool call: its kind's symbol, title and outcome; output and diffs unfold. */
@Composable private fun ToolRow(d: JsonObject) {
    var expanded by remember { mutableStateOf(false) }
    val icon = when (d.text("toolKind")) {
        "read" -> "doc.text"; "edit" -> "pencil"; "delete" -> "trash"; "move" -> "arrow.right.doc.on.clipboard"
        "search" -> "magnifyingglass"; "execute" -> "terminal"; "think" -> "brain"; "fetch" -> "globe"
        else -> "wrench.and.screwdriver"
    }
    val orb = when (d.text("toolKind")) { "search" -> ThinkingOrbState.Searching; "fetch" -> ThinkingOrbState.Connecting
        "edit" -> ThinkingOrbState.Shaping; else -> ThinkingOrbState.Working }
    val diffs = (d["diffs"] as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }
    val output = d.text("output").orEmpty()
    val label: @Composable () -> Unit = {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Box(Modifier.width(18.dp), contentAlignment = Alignment.Center) { Symbol(icon, size = 15.dp, tint = Palette.secondary) }
            Text(d.text("title") ?: "Tool", Modifier.weight(1f), style = Type.subheadline, color = Palette.text, maxLines = 2, overflow = TextOverflow.Ellipsis)
            when (d.text("status")) {
                "completed" -> Symbol("checkmark", size = 14.dp, tint = Palette.accent)
                "failed" -> Symbol("xmark", size = 14.dp, tint = Palette.danger)
                else -> ThinkingOrb(orb, 16.dp, Palette.secondary)
            }
        }
    }
    if (output.isEmpty() && diffs.isEmpty()) label()
    else Disclosure(expanded, { expanded = !expanded }, label = { label() }) {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            for (diff in diffs) DiffView(diff)
            if (output.isNotEmpty()) CodeBox(output)
        }
    }
}
