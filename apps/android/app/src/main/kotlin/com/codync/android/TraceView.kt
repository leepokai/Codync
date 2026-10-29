package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.Entry
import com.codync.android.design.*
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonPrimitive

/** "Full conversation": everything the agent did, grouped by turn (iOS `TraceView`). */
@Composable internal fun TraceView(bot: Bot, state: AppState) {
    val all = state.entries.filter { it.botId == bot.id }
    val turns = all.groupBy { it.turn }.toSortedMap().toList()
    // The running turn's thoughts start unfolded.
    val running = runningTurn(all, bot.isWorking(bot.id))
    val list = rememberLazyListState()
    // Opens at the end and follows any new step there (in whichever turn) until the reader scrolls up.
    rememberFollowing(list, turns.lastOrNull()?.first?.toString(), all.maxOfOrNull { it.rev }, working = false, bottom = 0.dp)
    Column(Modifier.fillMaxSize()) {
        ModalHeader("Full conversation")
        LazyColumn(state = list, contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(22.dp)) {
            if (turns.isEmpty()) item { Text(tr("Nothing yet."), style = Type.body, color = Palette.tertiary) }
            items(turns, key = { it.first }) { (turn, entries) ->
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(entries.firstOrNull { it.kind == "user" }?.text ?: "Turn $turn", Modifier.padding(start = 4.dp),
                        style = Type.subheadline.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    CardSection { for (entry in entries) row { TraceRow(entry, unfolded = turn == running) } }
                }
            }
        }
    }
}

/** The turn running now while the bot works in this chat; a queued message already holds the next turn. */
internal fun runningTurn(entries: List<Entry>, working: Boolean): Long? =
    if (working) entries.filter { it.status != "queued" }.maxOfOrNull { it.turn } else null

/** What the running turn has done so far, in order: the steps the working indicator unfolds. */
internal fun workingSteps(entries: List<Entry>): List<Entry> {
    val turn = runningTurn(entries, working = true)
    return entries.filter { it.turn == turn && it.isWorkingStep }
}

/** The running turn's thoughts start `unfolded`, so its reasoning reads as it comes in. */
@Composable internal fun TraceRow(entry: Entry, unfolded: Boolean) {
    val d = entry.data
    var expanded by rememberSaveable { mutableStateOf(unfolded) }
    when (entry.kind) {
        "user" -> Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Symbol("person.fill", size = 16.dp, tint = Palette.secondary)
            Text(entry.text, style = Type.subheadline, color = Palette.text)
        }
        "agent" -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(tr(if (d["final"]?.jsonPrimitive?.booleanOrNull == true) "Reply" else "Said"), style = Type.caption2.bold, color = Palette.tertiary)
            MarkdownText(entry.text)
        }
        "thought" -> Disclosure(expanded, { expanded = !expanded }, label = {
            ThinkingOrb(size = 16.dp, color = Palette.secondary, animated = false)
            Text(tr("Thinking"), Modifier.padding(start = 8.dp), style = Type.subheadline, color = Palette.secondary)
        }) { SelectionContainer { Text(entry.text, style = Type.footnote, color = Palette.secondary) } }
        "tool" -> ToolRow(d)
        "plan" -> Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Symbol("checklist", size = 16.dp, tint = Palette.text)
                Text(tr("Plan"), style = Type.subheadline.bold, color = Palette.text)
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
        else -> Text(tr(entry.text), style = Type.footnote, color = Palette.tertiary)
    }
}

/** A tool call: its kind's symbol, title and outcome; output and diffs unfold. */
@Composable private fun ToolRow(d: JsonObject) {
    var expanded by rememberSaveable { mutableStateOf(false) }
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
