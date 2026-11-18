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
import com.codync.android.core.Bot
import com.codync.android.core.Entry
import com.codync.android.core.LinkState
import com.codync.android.core.SourceKind
import com.codync.android.core.WorkAction
import com.codync.android.core.WorkItem
import com.codync.android.core.WhereLine
import com.codync.android.core.WorkStatus
import com.codync.android.core.whereLine
import com.codync.android.core.fill
import com.codync.android.core.givesWay
import com.codync.android.core.workActions
import com.codync.android.design.*
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

// The rows of a bot's timeline (`WorkTimeline`) and what they share with the capsule: an item's mark, its words and
// its time.

/**
 * The card a needs-you [item] waits on, among its bot's chat [entries]: the newest pending approval, choice or agent
 * sign-in card that names it (`data.item`) or its task (`data.task.id`), or, when that is a sign-in and another (the
 * agent the turn left signed out, while its fallback was too) holds the item's message, that one, whose sign-in sends
 * it again.
 */
internal fun pendingCard(entries: List<Entry>, item: WorkItem): Entry? {
    val cards = entries.filter { entry ->
        entry.waitsForUser &&
            (entry.data.string("item") == item.id || item.task != null && (entry.data["task"] as? JsonObject)?.string("id") == item.task)
    }
    val newest = cards.lastOrNull() ?: return null
    return if (newest.data["connectionRequest"] !is JsonObject) newest else cards.lastOrNull { it.holdsMessages } ?: newest
}

/** A sign-in card that holds messages: signing in there is what sends them again. */
private val Entry.holdsMessages: Boolean
    get() = ((data["connectionRequest"] as? JsonObject)?.get("entryIds") as? JsonArray)?.isNotEmpty() == true

/** A card waiting in the chat for the user: an approval, a choice, or a sign-in an agent needs. */
private val Entry.waitsForUser: Boolean get() = when (val request = data["connectionRequest"] as? JsonObject) {
    null -> (kind == "permission" || isDecision) && data.string("status") == "pending"
    else -> request.string("kind") == "agent" && request.string("status") == "pending"
}

private fun JsonObject.string(key: String): String? = (this[key] as? JsonPrimitive)?.contentOrNull

/** A needs-you item's second line: what its [card] asks (an approval's title, a choice's question, the agent a sign-in waits on). */
internal fun cardLine(card: Entry?, tr: (String) -> String): String? = card?.data?.let { data ->
    when (val request = data["connectionRequest"] as? JsonObject) {
        null -> data.string("title") ?: data.string("question")
        else -> fill(tr("Needs {agent} sign-in"), "agent" to request.string("title").orEmpty())
    }
}?.takeIf(String::isNotEmpty)

/**
 * A bot's work rows with what they need around them (the desktop's `LiveWork`): each row's actions and the Stop and
 * Cut in dialogs. [content] places the rows: its `rows(items, numbered)` adds one per item to a card, [numbered] with
 * its turn in the mark's place. A row that needs the user goes to its card in the chat ([openCard], which finds it there;
 * null where that chat isn't at hand). Any other row of an item a task has or had opens its thread ([openThread]).
 */
@Composable internal fun WorkRowsHost(bot: Bot, state: AppState, store: AppStore, openCard: ((WorkItem) -> Unit)?, openThread: (WorkItem) -> Unit,
    content: @Composable (rows: CardRows.(List<WorkItem>, Boolean) -> Unit) -> Unit) {
    // A change reaches the computer one at a time, and only while it's there.
    val enabled = state.actualConnection is LinkState.Ready && workKey(bot.id) !in state.busy
    val chat = state.entries.filter { it.botId == bot.id }
    /** The item Stop or Cut in asks about. */
    var stopping by remember(bot.id) { mutableStateOf<WorkItem?>(null) }
    var cuttingIn by remember(bot.id) { mutableStateOf<WorkItem?>(null) }
    fun run(action: WorkAction, item: WorkItem) = when (action) {
        WorkAction.Stop -> stopping = item
        WorkAction.CutIn -> cuttingIn = item
        WorkAction.First -> store.workFirst(bot.id, item.id)
        WorkAction.Drop -> store.dropWork(bot.id, item.id)
        WorkAction.Retry -> store.retryWork(bot.id, item.id)
    }
    content { items, numbered ->
        for ((index, item) in items.withIndex()) row {
            val card = if (item.status == WorkStatus.NeedsYou) pendingCard(chat, item) else null
            val open: (() -> Unit)? = when {
                item.status == WorkStatus.NeedsYou && openCard != null -> { { openCard(item) } }
                else -> item.task?.let { { openThread(item) } }
            }
            WorkRow(item, cardLine(card, translator()) ?: itemLine(item, translator()), workActions(item, bot.work, bot.maxAgents), enabled, open, state.home,
                number = (index + 1).takeIf { numbered }, source = { SourceMark(item, state.bots) }) { run(it, item) }
        }
    }
    StopWorkDialog(stopping != null, { stopping = null }) { stopping?.let { store.stopWork(bot.id, it.id) } }
    CutInDialog(cuttingIn, bot.work.filter(::givesWay), { cuttingIn = null }) { item, insteadOf -> store.workCutIn(bot.id, item.id, insteadOf.id) }
}

/**
 * One item (iOS `WorkRow`): its mark, title, what goes on ([subtitle]) and where it works, its time, and the action that matters most
 * (the first of [actions]); all of them are in its touch-and-hold menu. Tapping it runs [open]. [home]: the computer's, which its folder shows as `~`. [number]: its turn in
 * the event box, in the mark's place. [source]: where it came from, before its title.
 */
@Composable internal fun WorkRow(item: WorkItem, subtitle: String, actions: List<WorkAction>, enabled: Boolean, open: (() -> Unit)?, home: String,
    number: Int? = null, source: @Composable () -> Unit = {}, run: (WorkAction) -> Unit) {
    val ended = item.status.ended
    Row(Modifier.fillMaxWidth().heightIn(min = 36.dp).contextActions(onClick = open) {
        actions.map { action -> MenuItem(action.title, icon = action.symbol, destructive = action.destructive, enabled = enabled) { run(action) } }
    }, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            if (number == null) WorkMark(item.status)
            else Text("$number", Modifier.widthIn(min = 16.dp), style = Type.footnote.monospacedDigit, color = Palette.tertiary, textAlign = TextAlign.Center)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                    source()
                    Text(item.title, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Text(subtitle, style = Type.footnote, color = if (ended && item.status != WorkStatus.Done) Palette.danger else Palette.secondary,
                    maxLines = 2, overflow = TextOverflow.Ellipsis)
                whereLine(item, home)?.let { WhereLabel(it) }
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

/**
 * Where an item came from, before its title: the routine symbol for a routine's run, the asking bot's small avatar
 * for a request (one of this computer's [bots]); nothing for the bot's own work or an asker that isn't here.
 */
@Composable private fun SourceMark(item: WorkItem, bots: List<Bot>) {
    val source = item.source ?: return
    if (source.kind == SourceKind.Routine) return Symbol("clock.arrow.circlepath", size = 13.dp, tint = Palette.secondary, contentDescription = tr("Routine"))
    val asking = bots.firstOrNull { it.id == source.id && !it.deleted } ?: return
    Box(Modifier.semantics { contentDescription = asking.name }) { BotAvatar(asking, bots, 14.dp, animated = false) }
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

/**
 * Where an item's task works ([whereLine]), on its row and in its thread's header: the folder or the branch symbol, then
 * [text] (the folder or the branch unless said), cut in the middle when it doesn't fit.
 */
@Composable internal fun WhereLabel(where: WhereLine, modifier: Modifier = Modifier, text: String = where.text) {
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Symbol(where.kind.symbol, size = 12.dp, tint = Palette.tertiary)
        Text(text, style = Type.caption, color = Palette.tertiary, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
    }
}

/** How long a task has had an item (iOS `WorkElapsed`): counting while it works, fixed once it ended. */
@Composable internal fun WorkElapsed(item: WorkItem) {
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
