package com.codync.android

import android.content.ClipData
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.platform.testTag
import com.codync.android.core.isToldToThread
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.Entry
import com.codync.android.core.LinkState
import com.codync.android.core.WhereLine
import com.codync.android.core.WorkTask
import com.codync.android.core.sharedLine
import com.codync.android.core.sharing
import com.codync.android.core.threadDraftKey
import com.codync.android.core.threadTranscript
import com.codync.android.core.whereLine
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/**
 * The thread of [bot]'s task [taskId] (iOS `WorkThreadView`), pushed over the bot's details: its header (title, status,
 * time, where it works, Stop), what it did, with one quiet line where it put something in the bot's chat (a card,
 * a file), oldest first and following the newest, and a composer for the tasks that take the user's messages. The
 * transcript is fetched as the page opens and kept current from the computer's events (`AppStore.loadThread`). [back]
 * returns to the details; [openInChat] closes them and shows the chat at an entry the thread put there.
 */
@Composable internal fun WorkThreadView(bot: Bot, taskId: String, state: AppState, store: AppStore, openInChat: (Entry) -> Unit,
    back: () -> Unit) {
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Thread))
    // The task as it is now, kept as last seen once it leaves the box.
    var work by remember(taskId) { mutableStateOf<WorkTask?>(null) }
    bot.work.firstOrNull { it.id == taskId }?.let { if (it != work) work = it }
    val task = work
    // A task already gone from the box when its thread is asked for has nothing to show.
    LaunchedEffect(task == null) { if (task == null) back() }
    if (task == null) return
    val threadId = task.thread
    val live = task.status.live
    var loaded by remember(threadId) { mutableStateOf(threadId == null) }
    var error by remember(threadId) { mutableStateOf<String?>(null) }
    var attempt by remember(threadId) { mutableIntStateOf(0) }
    // Read again when the link comes back (its catch-up carries no thread entries), when its hello says another host
    // version (the transcript is asked for by it) and when the thread's turn ends (its last rows).
    LaunchedEffect(threadId, attempt, state.actualConnection is LinkState.Ready, state.hello?.text("version"), live) {
        if (threadId == null) return@LaunchedEffect
        try { store.loadThread(bot.id, threadId); loaded = true; error = null }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "Couldn't complete the action." }
    }
    DisposableEffect(threadId) { onDispose { threadId?.let { store.closeThread(it) } } }
    val all = state.threadEntries[threadId].orEmpty()
    val thread = threadTranscript(all)
    // As in a chat: while the thread's turn runs, what the user wrote waits over the composer, not among the rows.
    val queued = queuedMessages(thread.rest, live)
    val own = if (live) thread.rest.filterNot(Entry::isQueued) else thread.rest
    val rows = if (threadId == null) own else sharing(own, state.entries.filter { it.botId == bot.id && !it.isRemoved }, threadId)
    val sends = state.threadSends[taskId].orEmpty().filter { send -> all.none { it.clientNonce == send.nonce } }
    val running = runningTurn(all, live)
    val list = rememberLazyListState()
    var confirming by remember { mutableStateOf(false) }
    var composerHeight by remember { mutableIntStateOf(0) }
    val density = LocalDensity.current
    Column(Modifier.fillMaxSize().background(Palette.background).semantics { testTagsAsResourceId = true }) {
        ThreadHeader(task, state.home, stopping = workKey(bot.id) in state.busy, back) { confirming = true }
        GlassStage(Modifier.weight(1f).fillMaxWidth(), chrome = {
            Column(Modifier.align(Alignment.BottomCenter).fillMaxWidth().onSizeChanged { composerHeight = it.height }) {
                val readOnly = readOnlyLine(task)
                if (task.talk) ThreadComposer(state, store, threadDraftKey(bot.id, taskId), queued, steer = { store.steerThread(bot.id, taskId, it) },
                    remove = { store.unqueueThread(bot.id, taskId, it) }) { store.sendToThread(bot.id, taskId, it) }
                else if (readOnly != null) Text(tr(readOnly), Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 14.dp),
                    style = Type.footnote, color = Palette.tertiary, textAlign = TextAlign.Center)
            }
        }) {
            ConversationList(list, last = sends.lastOrNull()?.nonce ?: rows.lastOrNull()?.id, rev = all.maxOfOrNull { it.rev }, working = live,
                padding = PaddingValues(start = 16.dp, end = 16.dp, top = 8.dp, bottom = with(density) { composerHeight.toDp() } + 8.dp)) {
                thread.told?.let { told -> item("told") { Told(told.text) } }
                val failure = error
                when {
                    failure != null -> item("note") { LoadFailure(failure) { attempt++ } }
                    !loaded -> item("note") { Loading() }
                    all.isEmpty() -> item("note") { Text(tr("Nothing yet."), Modifier.padding(top = 10.dp), style = Type.body, color = Palette.tertiary) }
                }
                items(rows, key = { it.id }) { entry ->
                    // The chat's entry stays in the chat, answered and downloaded there.
                    if (entry.botId == bot.id) SharedRow(entry) { openInChat(entry) }
                    else Box(Modifier.padding(top = 10.dp)) { ThreadRow(entry, unfolded = entry.turn == running) }
                }
                items(sends, key = { "send-" + it.nonce }) { send ->
                    PendingBubble(send, groupStart = true, state, store, retry = { store.deliverToThread(taskId, send) },
                        discard = { store.discardThreadSend(taskId, send.nonce) })
                }
            }
        }
    }
    StopWorkDialog(confirming, { confirming = false }) { store.stopWork(bot.id, taskId) }
}

/**
 * Back to the details, the task's title over one quiet line (its status and how long its thread has run, then where it
 * works), and Stop while a thread has it.
 */
@Composable private fun ThreadHeader(task: WorkTask, home: String, stopping: Boolean, back: () -> Unit, stop: () -> Unit) {
    Row(Modifier.fillMaxWidth().padding(start = 10.dp, end = 12.dp, top = 12.dp, bottom = 6.dp).heightIn(min = 44.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        IconButton("Back to details", "chevron.left", action = back)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(task.title, Modifier.semantics { heading() }, style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                Row(Modifier.testTag("thread-status").semantics(mergeDescendants = true) {}, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                    // The word beside it says the same.
                    Box(Modifier.clearAndSetSemantics {}) { WorkMark(task.status) }
                    Text(tr(task.status.title), style = Type.footnote, color = Palette.secondary, maxLines = 1)
                    WorkElapsed(task)
                }
                whereLine(task, home)?.let { WherePath(it, Modifier.weight(1f, fill = false)) }
            }
        }
        if (task.status.live) IconButton("Stop", "stop.fill", enabled = !stopping, action = stop)
    }
}

/** Where the thread works, as its row says it; a tap copies the full path and says so for a moment. */
@Composable private fun WherePath(where: WhereLine, modifier: Modifier) {
    val path = where.path ?: return WhereLabel(where, modifier)
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    var copied by remember { mutableStateOf(false) }
    val copy = tr("Copy path")
    WhereLabel(where, modifier.testTag("thread-where").clickable(role = Role.Button, onClickLabel = copy) {
        scope.launch {
            clipboard.setClipEntry(ClipEntry(ClipData.newPlainText("Path", path)))
            copied = true; delay(1500); copied = false
        }
    }.semantics { liveRegion = LiveRegionMode.Polite }, text = if (copied) tr("Path copied") else where.text)
}

/** What the host told the thread first (the task's prompt), folded on top of its thread. */
@Composable private fun Told(told: String) {
    var expanded by rememberSaveable { mutableStateOf(false) }
    Disclosure(expanded, { expanded = !expanded }, Modifier.fillMaxWidth(), label = {
        Text(tr("What it was told"), style = Type.footnote.medium, color = Palette.secondary)
    }) {
        SelectionContainer {
            Text(told, Modifier.fillMaxWidth().clip(RoundedRectangle(12.dp)).background(Palette.bubbleAgent).padding(12.dp),
                style = Type.footnote, color = Palette.secondary)
        }
    }
}

/**
 * A row of a thread: the user's messages and the thread's replies as the chat's bubbles, a step between them as the chat's
 * working steps show it, and what the host told the thread later (a session that starts again) as a quiet quote, never a
 * message of the user's.
 */
@Composable private fun ThreadRow(entry: Entry, unfolded: Boolean) {
    if (entry.isToldToThread) SelectionContainer {
        Text("“${entry.text}”", Modifier.padding(horizontal = 4.dp), style = Type.subheadline, color = Palette.secondary)
    } else if (entry.kind == "user" || entry.kind == "agent" && entry.isChat) PlainMessageRow(entry)
    else Box(Modifier.padding(horizontal = 4.dp)) { TraceRow(entry, unfolded) }
}

/** What the thread put in the bot's chat, as one quiet line ([sharedLine]); a tap shows it in the chat. */
@Composable private fun SharedRow(entry: Entry, open: () -> Unit) {
    val line = sharedLine(entry, translator()) ?: return
    val show = tr("Show in the chat")
    Row(Modifier.fillMaxWidth().padding(top = 10.dp).clip(RoundedRectangle(8.dp)).clickable(role = Role.Button, onClickLabel = show, onClick = open)
        .padding(horizontal = 4.dp, vertical = 2.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Symbol(line.symbol, size = 12.dp, tint = Palette.secondary)
        Text(line.text, style = Type.footnote, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

@Composable private fun Loading() {
    val label = tr("Loading")
    Box(Modifier.fillMaxWidth().padding(top = 10.dp).clearAndSetSemantics { contentDescription = label }, contentAlignment = Alignment.Center) { Spinner(14.dp) }
}

@Composable private fun LoadFailure(message: String, retry: () -> Unit) {
    Column(Modifier.fillMaxWidth().padding(top = 10.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(tr(message), style = Type.footnote, color = Palette.danger, textAlign = TextAlign.Center)
        LinkButton("Try again", action = retry)
    }
}
