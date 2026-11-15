package com.codync.android

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
import com.codync.android.core.Entry
import com.codync.android.core.LinkState
import com.codync.android.core.WorkItem
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException

/**
 * The transcript of the task working [itemId], or the one that had it, oldest first: fetched as it opens and kept
 * current from the computer's events (`AppStore.loadTask`). Opened from the item's row in the bot's details.
 */
@Composable internal fun TaskDetailsView(bot: Bot, itemId: String, taskId: String, state: AppState, store: AppStore) {
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Task))
    // The item as it is now, kept as last seen once it leaves the box.
    var work by remember(itemId) { mutableStateOf<WorkItem?>(null) }
    bot.work.firstOrNull { it.id == itemId }?.let { if (it != work) work = it }
    val live = work?.status?.live == true && work?.task == taskId
    var loaded by remember(taskId) { mutableStateOf(false) }
    var error by remember(taskId) { mutableStateOf<String?>(null) }
    var attempt by remember(taskId) { mutableIntStateOf(0) }
    // Read again when the link comes back (its catch-up carries no task entries) and when the task's turn ends (its last rows).
    LaunchedEffect(taskId, attempt, state.actualConnection is LinkState.Ready, live) {
        try { store.loadTask(bot.id, taskId); loaded = true; error = null }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "Couldn't complete the action." }
    }
    DisposableEffect(taskId) { onDispose { store.closeTask(taskId) } }
    val entries = state.taskEntries[taskId].orEmpty()
    val turns = entries.groupBy { it.turn }.toSortedMap().toList()
    val running = runningTurn(entries, live)
    val list = rememberLazyListState()
    rememberFollowing(list, turns.lastOrNull()?.first?.toString(), entries.maxOfOrNull { it.rev }, working = false, bottom = 0.dp)
    var confirming by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(title = {
            Text(work?.title.orEmpty(), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }, trailing = {
            if (live) IconButton("Stop", "stop.fill", enabled = workKey(bot.id) !in state.busy) { confirming = true }
        })
        work?.let { TaskStatusLine(it) }
        work?.worktree?.let { BranchLine(it.branch) }
        LazyColumn(state = list, contentPadding = PaddingValues(20.dp), verticalArrangement = Arrangement.spacedBy(22.dp)) {
            val failure = error
            when {
                failure != null -> item("note") { LoadFailure(failure) { attempt++ } }
                !loaded -> item("note") { Loading() }
                entries.isEmpty() -> item("note") { Text(tr("Nothing yet."), style = Type.body, color = Palette.tertiary) }
            }
            items(turns, key = { it.first }) { (turn, rows) ->
                CardSection { for (entry in rows) row { TaskEntryRow(entry, unfolded = turn == running) } }
            }
        }
    }
    StopWorkDialog(confirming, { confirming = false }) { store.stopWork(bot.id, itemId) }
}

/** Under the title: the item's status in words and how long its task has run, counting while it works and fixed once it ended. */
@Composable private fun TaskStatusLine(item: WorkItem) {
    val started = item.startedAt ?: return
    Row(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, bottom = 8.dp).semantics(mergeDescendants = true) {},
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(tr(item.status.title), style = Type.footnote, color = Palette.secondary, maxLines = 1)
        val ended = item.endedAt
        if (ended != null) Text(timerText(ended - started), style = Type.footnote.monospacedDigit, color = Palette.tertiary)
        else if (item.status.live) Elapsed(started)
    }
}

/** The branch of the worktree the task works in. */
@Composable private fun BranchLine(branch: String) {
    Row(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, bottom = 8.dp).semantics(mergeDescendants = true) {},
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
        Symbol("arrow.triangle.branch", size = 13.dp, tint = Palette.tertiary)
        Text(branch, style = Type.footnote.monospaced, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
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
