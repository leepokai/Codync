package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Entry
import com.codync.android.core.WorkAction
import com.codync.android.core.WorkItem
import com.codync.android.core.WorkStatus
import com.codync.android.core.fill
import com.codync.android.design.*
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

// The rows of a bot's timeline (`WorkTimeline`) and what they share with the capsule: an item's mark, its words and
// its time.

/**
 * The approval card a needs-you [item] waits on, among its bot's chat [entries]: the newest pending one that names it
 * (`data.item`) or its task (`data.task.id`).
 */
internal fun pendingCard(entries: List<Entry>, item: WorkItem): Entry? = entries.lastOrNull { entry ->
    entry.kind == "permission" && entry.data.string("status") == "pending" &&
        (entry.data.string("item") == item.id || item.task != null && (entry.data["task"] as? JsonObject)?.string("id") == item.task)
}

private fun JsonObject.string(key: String): String? = (this[key] as? JsonPrimitive)?.contentOrNull

/** A needs-you item's second line: what its [card] asks. */
internal fun cardLine(card: Entry?): String? = card?.data?.string("title")?.takeIf(String::isNotEmpty)

/**
 * One item (iOS `WorkRow`): its mark, title and what goes on ([subtitle]), its time, and the action that matters most
 * (the first of [actions]); all of them are in its touch-and-hold menu. Tapping it runs [open]. [number]: its turn in
 * the event box, in the mark's place.
 */
@Composable internal fun WorkRow(item: WorkItem, subtitle: String, actions: List<WorkAction>, enabled: Boolean, open: (() -> Unit)?,
    number: Int? = null, run: (WorkAction) -> Unit) {
    val ended = item.status.ended
    Row(Modifier.fillMaxWidth().heightIn(min = 36.dp).contextActions(onClick = open) {
        actions.map { action -> MenuItem(action.title, icon = action.symbol, destructive = action.destructive, enabled = enabled) { run(action) } }
    }, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            if (number == null) WorkMark(item.status)
            else Text("$number", Modifier.widthIn(min = 16.dp), style = Type.footnote.monospacedDigit, color = Palette.tertiary, textAlign = TextAlign.Center)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(item.title, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(subtitle, style = Type.footnote, color = if (ended && item.status != WorkStatus.Done) Palette.danger else Palette.secondary,
                    maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
            WorkElapsed(item)
            if (open != null && ended) Symbol("chevron.right", size = 13.dp, tint = Palette.tertiary)
        }
        val primary = actions.firstOrNull() ?: return@Row
        // Icon-only, but Cut in: no icon says it.
        if (primary == WorkAction.CutIn) Text(tr(primary.title), Modifier.plainButton(enabled) { run(primary) }.outline().pill(Color.Transparent),
            style = Type.footnote.medium, color = Palette.text)
        else IconButton(primary.title, primary.symbol, enabled = enabled) { run(primary) }
    }
}

/** A place of the bot's not in use, among its threads: one faint row, so the ceiling is visible. */
@Composable internal fun FreeRow() {
    Row(Modifier.fillMaxWidth().heightIn(min = 36.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Box(Modifier.size(16.dp), contentAlignment = Alignment.Center) { Box(Modifier.size(10.dp).border(1.5.dp, Palette.border, CircleShape)) }
        Text(tr("Free"), style = Type.body, color = Palette.tertiary)
    }
}

/** An item's status as a mark (iOS `WorkMark`): attention, a moving dot, a faint ring (waiting), a ring (done), the danger mark. */
@Composable internal fun WorkMark(status: WorkStatus) {
    val word = tr(status.title)
    Box(Modifier.size(16.dp).semantics { contentDescription = word }, contentAlignment = Alignment.Center) {
        when (status) {
            WorkStatus.NeedsYou -> ThinkingOrb(ThinkingOrbState.Listening, 14.dp, Palette.warning)
            WorkStatus.Working -> ThinkingOrb(size = 13.dp, color = Palette.secondary)
            WorkStatus.Waiting -> Box(Modifier.size(10.dp).border(1.5.dp, Palette.tertiary, CircleShape))
            WorkStatus.Done -> Box(Modifier.size(10.dp).border(1.5.dp, Palette.secondary, CircleShape))
            WorkStatus.Failed, WorkStatus.Stopped -> Box(Modifier.size(8.dp).background(Palette.danger, CircleShape))
        }
    }
}

/** How long a task has had an item (iOS `WorkElapsed`): counting while it works, fixed once it ended. */
@Composable private fun WorkElapsed(item: WorkItem) {
    val started = item.startedAt ?: return
    val ended = item.endedAt
    if (ended != null) Text(timerText(ended - started), style = Type.footnote.monospacedDigit, color = Palette.tertiary)
    else if (item.status.live) Elapsed(started)
}

/** Asks which of the [live] items gives way to [item] (null: closed): its title, what it does, how long. */
@Composable internal fun CutInDialog(item: WorkItem?, live: List<WorkItem>, dismiss: () -> Unit, cutIn: (WorkItem, WorkItem) -> Unit) {
    // The dialog keeps its item while it fades out.
    var shown by remember { mutableStateOf(item) }
    if (item != null && item != shown) shown = item
    val asking = shown ?: return
    val translate = translator()
    val now = System.currentTimeMillis()
    CodyncDialog(fill("Let \"{title}\" cut in", "title" to asking.title), item != null, dismiss,
        message = "Every place is taken. Pick what gives way: its session closes and it starts again later.") {
        live.map { giving -> DialogAction(cutInChoice(giving, now, translate), verbatim = true) { cutIn(asking, giving) } }
    }
}

/** A live item in the cut-in dialog: its title, what it does, how long. */
internal fun cutInChoice(item: WorkItem, now: Long, tr: (String) -> String): String = listOfNotNull(
    item.title,
    if (item.status == WorkStatus.NeedsYou) tr(item.status.title) else item.activity.takeIf(String::isNotEmpty)?.let(tr),
    item.startedAt?.let { timerText(now - it) },
).joinToString(" · ")
