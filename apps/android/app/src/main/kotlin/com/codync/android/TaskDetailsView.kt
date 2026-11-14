package com.codync.android

import androidx.activity.compose.BackHandler
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.BotTask
import com.codync.android.core.Entry
import com.codync.android.core.LinkState
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException

/**
 * What the bot is doing in the background, opened from the capsule (iOS `TasksSheet`), read only: one task opens
 * straight on its details, several on their list, where a row opens that task.
 */
@Composable internal fun TasksSheet(bot: Bot, state: AppState, store: AppStore) {
    val listed = remember { bot.tasks.size > 1 }
    var open by remember { mutableStateOf(bot.tasks.singleOrNull()?.id) }
    BackHandler(listed && open != null) { open = null }
    AnimatedContent(open, Modifier.fillMaxSize().background(Palette.background),
        transitionSpec = { fadeIn(Motion.fade()) togetherWith fadeOut(Motion.fade()) }, label = "Task") { id ->
        if (id == null) TaskList(bot.tasks) { open = it }
        else TaskDetails(bot, id, state, store, back = if (listed) { { open = null } } else null)
    }
}

/** The bot's tasks still going: title, status and time. */
@Composable private fun TaskList(tasks: List<BotTask>, open: (String) -> Unit) {
    Column(Modifier.fillMaxSize()) {
        ModalHeader("In the background")
        Column(Modifier.padding(20.dp)) {
            if (tasks.isEmpty()) Text(tr("Nothing yet."), style = Type.body, color = Palette.tertiary)
            else CardSection {
                for (task in tasks) row {
                    Row(Modifier.fillMaxWidth().plainButton { open(task.id) }.semantics(mergeDescendants = true) {},
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Text(task.title, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Text(tr(task.status.title), style = Type.footnote, color = Palette.secondary)
                        }
                        Elapsed(task.startedAt)
                        Symbol("chevron.right", size = 13.dp, tint = Palette.tertiary)
                    }
                }
            }
        }
    }
}

/**
 * One task's own transcript, oldest first: fetched as it opens and kept current from the computer's events
 * (`AppStore.loadTask`). [back] returns to the list it was opened from.
 */
@Composable private fun TaskDetails(bot: Bot, taskId: String, state: AppState, store: AppStore, back: (() -> Unit)?) {
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Task))
    val live = bot.tasks.firstOrNull { it.id == taskId }
    // The task as the computer answered: what is known of it once it has ended; null while it loads.
    var fetched by remember(taskId) { mutableStateOf<BotTask?>(null) }
    var error by remember(taskId) { mutableStateOf<String?>(null) }
    var attempt by remember(taskId) { mutableIntStateOf(0) }
    // Read again when the link comes back (its catch-up carries no task entries) and when the task ends (its last status).
    LaunchedEffect(taskId, attempt, state.actualConnection is LinkState.Ready, live != null) {
        try { fetched = store.loadTask(bot.id, taskId); error = null }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "Couldn't complete the action." }
    }
    DisposableEffect(taskId) { onDispose { store.closeTask(taskId) } }
    val task = live ?: fetched
    val entries = state.taskEntries[taskId].orEmpty()
    val turns = entries.groupBy { it.turn }.toSortedMap().toList()
    val running = runningTurn(entries, live != null)
    val list = rememberLazyListState()
    rememberFollowing(list, turns.lastOrNull()?.first?.toString(), entries.maxOfOrNull { it.rev }, working = false, bottom = 0.dp)
    var confirming by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxSize()) {
        ModalHeader(title = {
            if (back != null) IconButton("Back", "chevron.left", action = back)
            Text(task?.title.orEmpty(), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }, trailing = {
            if (live != null) IconButton("Stop", "stop.fill", enabled = stopTaskKey(bot.id) !in state.busy) { confirming = true }
        })
        if (task != null) TaskStatusLine(task)
        LazyColumn(state = list, contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(22.dp)) {
            val failure = error
            when {
                failure != null -> item("note") { LoadFailure(failure) { attempt++ } }
                fetched == null -> item("note") { Loading() }
                entries.isEmpty() -> item("note") { Text(tr("Nothing yet."), style = Type.body, color = Palette.tertiary) }
            }
            items(turns, key = { it.first }) { (turn, rows) ->
                CardSection { for (entry in rows) row { TaskEntryRow(entry, unfolded = turn == running) } }
            }
        }
    }
    StopTasksDialog(confirming, { confirming = false }) { store.stopTask(bot.id, taskId) }
}

/** Under the title: the task's status in words and how long it has run, counting while it works and fixed once it ended. */
@Composable private fun TaskStatusLine(task: BotTask) {
    Row(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, bottom = 8.dp).semantics(mergeDescendants = true) {},
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(tr(task.status.title), style = Type.footnote, color = Palette.secondary, maxLines = 1)
        val ended = task.endedAt
        if (ended != null) Text(timerText(ended - task.startedAt), style = Type.footnote.monospacedDigit, color = Palette.tertiary)
        else if (task.status.live) Elapsed(task.startedAt)
    }
}

@Composable private fun Loading() {
    val label = tr("Loading")
    Box(Modifier.fillMaxWidth().clearAndSetSemantics { contentDescription = label }, contentAlignment = Alignment.Center) { Spinner(14.dp) }
}

@Composable private fun LoadFailure(message: String, retry: () -> Unit) {
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(tr(message), style = Type.footnote, color = Palette.danger, textAlign = TextAlign.Center)
        LinkButton("Try again", action = retry)
    }
}

/** A row of the trace, except what the task was told (its bot's words, sent by the host): quoted text, not a message of the user's. */
@Composable private fun TaskEntryRow(entry: Entry, unfolded: Boolean) {
    if (entry.kind == "user" && entry.data.text("from") == "host") SelectionContainer {
        Text("“${entry.text}”", style = Type.subheadline, color = Palette.secondary)
    } else TraceRow(entry, unfolded)
}
