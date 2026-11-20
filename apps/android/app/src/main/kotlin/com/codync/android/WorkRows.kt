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
import com.codync.android.core.WorkTask
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

// The rows of a bot's timeline (`WorkTimeline`) and what they share with the capsule: a task's mark, its words and
// its time.

/**
 * The card a needs-you [task] waits on, among its bot's chat [entries]: the newest pending approval, choice or agent
 * sign-in card that names it (`data.task`) or its thread (`data.thread.id`), or, when that is a sign-in and another (the
 * agent the turn left signed out, while its fallback was too) holds the task's message, that one, whose sign-in sends
 * it again.
 */
internal fun pendingCard(entries: List<Entry>, task: WorkTask): Entry? {
    val cards = entries.filter { entry ->
        entry.waitsForUser &&
            (entry.data.string("task") == task.id || task.thread != null && (entry.data["thread"] as? JsonObject)?.string("id") == task.thread)
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

/** A needs-you task's second line: what its [card] asks (an approval's title, a choice's question, the agent a sign-in waits on). */
internal fun cardLine(card: Entry?, tr: (String) -> String): String? = card?.data?.let { data ->
    when (val request = data["connectionRequest"] as? JsonObject) {
        null -> data.string("title") ?: data.string("question")
        else -> fill(tr("Needs {agent} sign-in"), "agent" to request.string("title").orEmpty())
    }
}?.takeIf(String::isNotEmpty)

/**
 * A bot's work rows with what they need around them (the desktop's `LiveWork`): each row's actions and the Stop and
 * Cut in dialogs. [content] places the rows: its `rows(tasks, numbered)` adds one per task to a card, [numbered] with
 * its turn in the mark's place. A row that needs the user goes to its card in the chat ([openCard], which finds it there;
 * null where that chat isn't at hand). Any other row of a task a thread has or had opens its thread ([openThread]).
 */
@Composable internal fun WorkRowsHost(bot: Bot, state: AppState, store: AppStore, openCard: ((WorkTask) -> Unit)?, openThread: (WorkTask) -> Unit,
    content: @Composable (rows: CardRows.(List<WorkTask>, Boolean) -> Unit) -> Unit) {
    // A change reaches the computer one at a time, and only while it's there.
    val enabled = state.actualConnection is LinkState.Ready && workKey(bot.id) !in state.busy
    val chat = state.entries.filter { it.botId == bot.id }
    /** The task Stop or Cut in asks about. */
    var stopping by remember(bot.id) { mutableStateOf<WorkTask?>(null) }
    var cuttingIn by remember(bot.id) { mutableStateOf<WorkTask?>(null) }
    fun run(action: WorkAction, task: WorkTask) = when (action) {
        WorkAction.Stop -> stopping = task
        WorkAction.CutIn -> cuttingIn = task
        WorkAction.First -> store.workFirst(bot.id, task.id)
        WorkAction.Drop -> store.dropWork(bot.id, task.id)
        WorkAction.Retry -> store.retryWork(bot.id, task.id)
    }
    content { tasks, numbered ->
        for ((index, task) in tasks.withIndex()) row {
            val card = if (task.status == WorkStatus.NeedsYou) pendingCard(chat, task) else null
            val open: (() -> Unit)? = when {
                task.status == WorkStatus.NeedsYou && openCard != null -> { { openCard(task) } }
                else -> task.thread?.let { { openThread(task) } }
            }
            WorkRow(task, cardLine(card, translator()) ?: taskLine(task, translator()), workActions(task, bot.work, bot.maxThreads), enabled, open, state.home,
                number = (index + 1).takeIf { numbered }, source = { SourceMark(task, state.bots) }) { run(it, task) }
        }
    }
    StopWorkDialog(stopping != null, { stopping = null }) { stopping?.let { store.stopWork(bot.id, it.id) } }
    CutInDialog(cuttingIn, bot.work.filter(::givesWay), { cuttingIn = null }) { task, insteadOf -> store.workCutIn(bot.id, task.id, insteadOf.id) }
}

/**
 * One task (iOS `WorkRow`): its mark, title, what goes on ([subtitle]) and where it works, its time, and the action that matters most
 * (the first of [actions]); all of them are in its touch-and-hold menu. Tapping it runs [open]. [home]: the computer's, which its folder shows as `~`. [number]: its turn in
 * the event box, in the mark's place. [source]: where it came from, before its title.
 */
@Composable internal fun WorkRow(task: WorkTask, subtitle: String, actions: List<WorkAction>, enabled: Boolean, open: (() -> Unit)?, home: String,
    number: Int? = null, source: @Composable () -> Unit = {}, run: (WorkAction) -> Unit) {
    val ended = task.status.ended
    Row(Modifier.fillMaxWidth().heightIn(min = 36.dp).contextActions(onClick = open) {
        actions.map { action -> MenuItem(action.title, icon = action.symbol, destructive = action.destructive, enabled = enabled) { run(action) } }
    }, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            if (number == null) WorkMark(task.status)
            else Text("$number", Modifier.widthIn(min = 16.dp), style = Type.footnote.monospacedDigit, color = Palette.tertiary, textAlign = TextAlign.Center)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                    source()
                    Text(task.title, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Text(subtitle, style = Type.footnote, color = if (ended && task.status != WorkStatus.Done) Palette.danger else Palette.secondary,
                    maxLines = 2, overflow = TextOverflow.Ellipsis)
                whereLine(task, home)?.let { WhereLabel(it) }
            }
            WorkElapsed(task)
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
 * Where a task came from, before its title: the routine symbol for a routine's run, the asking bot's small avatar
 * for a request (one of this computer's [bots]); nothing for the bot's own work or an asker that isn't here.
 */
@Composable private fun SourceMark(task: WorkTask, bots: List<Bot>) {
    val source = task.source ?: return
    if (source.kind == SourceKind.Routine) return Symbol("clock.arrow.circlepath", size = 13.dp, tint = Palette.secondary, contentDescription = tr("Routine"))
    val asking = bots.firstOrNull { it.id == source.id && !it.deleted } ?: return
    Box(Modifier.semantics { contentDescription = asking.name }) { BotAvatar(asking, bots, 14.dp, animated = false) }
}

/** A thread the bot could still start, among the ones that run: one faint row, so the ceiling is visible. */
@Composable internal fun FreeRow() {
    Row(Modifier.fillMaxWidth().heightIn(min = 36.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Box(Modifier.size(16.dp), contentAlignment = Alignment.Center) { Box(Modifier.size(10.dp).border(1.5.dp, Palette.border, CircleShape)) }
        Text(tr("Free"), style = Type.body, color = Palette.tertiary)
    }
}

/** A task's status as a mark (iOS `WorkMark`): attention, a moving dot, a faint ring (waiting), a ring (done), the danger mark. */
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
 * Where a task's thread works ([whereLine]), on its row and in its thread's header: the folder or the branch symbol, then
 * [text] (the folder or the branch unless said), cut in the middle when it doesn't fit.
 */
@Composable internal fun WhereLabel(where: WhereLine, modifier: Modifier = Modifier, text: String = where.text) {
    Row(modifier, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Symbol(where.kind.symbol, size = 12.dp, tint = Palette.tertiary)
        Text(text, style = Type.caption, color = Palette.tertiary, maxLines = 1, overflow = TextOverflow.MiddleEllipsis)
    }
}

/** How long a thread has had a task (iOS `WorkElapsed`): counting while it works, fixed once it ended. */
@Composable internal fun WorkElapsed(task: WorkTask) {
    val started = task.startedAt ?: return
    val ended = task.endedAt
    if (ended != null) Text(timerText(ended - started), style = Type.footnote.monospacedDigit, color = Palette.tertiary)
    else if (task.status.live) Elapsed(started)
}

/** Asks which of the [live] tasks gives way to [task] (null: closed): its title, what it does, how long. */
@Composable internal fun CutInDialog(task: WorkTask?, live: List<WorkTask>, dismiss: () -> Unit, cutIn: (WorkTask, WorkTask) -> Unit) {
    // The dialog keeps its task while it fades out.
    var shown by remember { mutableStateOf(task) }
    if (task != null && task != shown) shown = task
    val asking = shown ?: return
    val translate = translator()
    val now = System.currentTimeMillis()
    CodyncDialog(fill("Let \"{title}\" cut in", "title" to asking.title), task != null, dismiss,
        message = "Every thread is busy. Pick the task that gives way: its thread closes and the task starts again later.") {
        live.map { giving -> DialogAction(cutInChoice(giving, now, translate), verbatim = true) { cutIn(asking, giving) } }
    }
}

/** A live task in the cut-in dialog: its title, what it does, how long. */
internal fun cutInChoice(task: WorkTask, now: Long, tr: (String) -> String): String = listOfNotNull(
    task.title,
    if (task.status == WorkStatus.NeedsYou) tr(task.status.title) else task.activity.takeIf(String::isNotEmpty)?.let(tr),
    task.startedAt?.let { timerText(now - it) },
).joinToString(" · ")
