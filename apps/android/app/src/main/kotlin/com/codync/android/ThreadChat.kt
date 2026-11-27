package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
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
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.codync.android.core.Bot
import com.codync.android.core.ComposioApp
import com.codync.android.core.MarketConnector
import com.codync.android.core.Entry
import com.codync.android.core.LinkState
import com.codync.android.core.PendingSend
import com.codync.android.core.WhereLine
import com.codync.android.core.WorkStatus
import com.codync.android.core.WorkTask
import com.codync.android.core.hidingQueue
import com.codync.android.core.isToldToThread
import com.codync.android.core.queue
import com.codync.android.core.sharing
import com.codync.android.core.threadDraftKey
import com.codync.android.core.threadTranscript
import com.codync.android.core.whereLine
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.CancellationException

/**
 * The thread of [bot]'s task [taskId] in the chat's own form (the desktop's `ThreadChat`, iOS `ThreadChatView`): the
 * user's messages and the thread's replies as the chat's bubbles, what it put in the bot's chat (a card to answer, a file,
 * an artifact) as the chat's own rows, what it was told folded on top, and while it works the chat's working line with
 * its steps. On top: Back, the bot (a thread without a bot: its agent) and the title over how it stands, and Details
 * ([openDetails], the bot's details; none for a thread without a bot, nor [inDetails], pushed inside them). Under it: the
 * chat's composer writing to the thread, with the agent it runs on and where it works; for a routine's run or another
 * bot's request, why it is read only, and Stop while it runs. The transcript is fetched as it opens and kept current from
 * the computer's events (`AppStore.loadThread`).
 */
@Composable internal fun ThreadChat(bot: Bot, taskId: String, state: AppState, store: AppStore, inDetails: Boolean,
    openDetails: ((DetailsRequest) -> Unit)?, openArtifact: (OpenArtifact) -> Unit, back: () -> Unit) {
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
    val online = state.actualConnection is LinkState.Ready
    var loaded by remember(threadId) { mutableStateOf(threadId == null) }
    var error by remember(threadId) { mutableStateOf<String?>(null) }
    var attempt by remember(threadId) { mutableIntStateOf(0) }
    // Read again when the link comes back (its catch-up carries no thread entries), when its hello says another host
    // version (the transcript is asked for by it) and when the thread's turn ends (its last rows).
    LaunchedEffect(threadId, attempt, online, state.hello?.text("version"), live) {
        if (threadId == null) return@LaunchedEffect
        try { store.loadThread(bot.id, threadId); loaded = true; error = null }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "Couldn't complete the action." }
    }
    DisposableEffect(threadId) {
        threadId?.let(store::watchThread)
        onDispose { threadId?.let(store::closeThread) }
    }
    // What it put in the bot's chat, and the card it waits on, maybe further back than the rows loaded.
    val needsYou = task.status == WorkStatus.NeedsYou
    // Again once the chat's own paging (or the app's loading) ends: [holdChat] doesn't start while it runs.
    var held by remember(bot.id, needsYou) { mutableStateOf(false) }
    LaunchedEffect(bot.id, needsYou, state.loading, historyKey(bot.id) in state.busy) { if (!held) held = store.holdChat(bot.id, task.takeIf { needsYou }) }
    val all = state.threadEntries[threadId].orEmpty()
    val thread = threadTranscript(all)
    // As in a chat: while the thread's turn runs, what the user wrote waits over the composer, not among the rows.
    val queued = thread.queue(live)
    val items = threadChatItems(all, state.entries.filter { it.botId == bot.id }, threadId, state.threadSends[taskId].orEmpty(), live)
    val details by ThinkingDetails.enabled.collectAsStateWithLifecycle()
    val list = rememberLazyListState()
    var confirming by remember { mutableStateOf(false) }
    var peer by remember { mutableStateOf<String?>(null) }
    var setupEntry by rememberSaveable { mutableStateOf<String?>(null) }
    var app by remember { mutableStateOf<ComposioApp?>(null) }
    var connector by remember { mutableStateOf<MarketConnector?>(null) }
    val connect = ConnectionRequestActions({ id, item -> setupEntry = id; app = item }, { id, item -> setupEntry = id; connector = item })
    val addFallback = openDetails?.let { open -> { owner: Bot -> open(DetailsRequest.fallbacks(owner.id)) } }
    val talks = task.talk && threadId != null
    var composerHeight by remember { mutableIntStateOf(0) }
    val density = LocalDensity.current
    // Over the Threads tab it fills the screen and the chat scrolls under the status bar; in the bot's details, its sheet.
    val top = if (inDetails) 0.dp else WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    // As the bot's chat: the header's glass buttons float over the rows, which blur away under it and over the composer.
    GlassStage(Modifier.fillMaxSize().background(Palette.background).semantics { testTagsAsResourceId = true }, chrome = {
        ScrollEdge(Edge.Top, top + 84.dp)
        ScrollEdge(Edge.Bottom, with(density) { composerHeight.toDp() } + 4.dp, Modifier.align(Alignment.BottomCenter))
        ThreadHeader(bot, task, state, showsWhere = !talks, back, openDetails?.takeIf { !inDetails }, Modifier.padding(top = top))
        Column(Modifier.align(Alignment.BottomCenter).fillMaxWidth().onSizeChanged { composerHeight = it.height }
            .then(if (inDetails) Modifier else Modifier.imePadding().navigationBarsPadding())) {
            ThreadFooter(bot, task, state, store, talks, queued, online) { confirming = true }
        }
    }) {
            // In the thread's own chat, the "Work · <title>" line under its cards says nothing new (iOS `WorkThreadLine`).
            androidx.compose.runtime.CompositionLocalProvider(LocalInThread provides true) {
            ConversationList(list, first = items.firstOrNull()?.id, last = items.lastOrNull()?.id, rev = all.maxOfOrNull { it.rev }, working = live,
                padding = PaddingValues(start = 16.dp, end = 16.dp, top = top + 76.dp, bottom = with(density) { composerHeight.toDp() } + 8.dp)) {
                val failure = error
                when {
                    failure != null -> item("note") { LoadFailure(failure) { attempt++ } }
                    !loaded -> item("note") { Loading() }
                    items.isEmpty() -> item("note") {
                        Text(tr("Nothing yet."), Modifier.fillMaxWidth().padding(top = 10.dp), style = Type.footnote, color = Palette.tertiary, textAlign = TextAlign.Center)
                    }
                }
                items(items, key = { it.id }) { item ->
                    when (item) {
                        is ChatItem.Separator -> TimeSeparator(item.at)
                        is ChatItem.Message -> ThreadMessage(item, thread.told?.id, bot, state, store, connect, addFallback, openArtifact)
                        is ChatItem.Pending -> PendingBubble(item.send, item.groupStart, state, store, retry = { store.deliverToThread(taskId, item.send) },
                            discard = { store.discardThreadSend(taskId, item.send.nonce) })
                        is ChatItem.Exchanges -> BotMessageRow(item.group, state) { peer = item.group.peerId }
                    }
                }
                // Offline, "working" is only what the computer last said; it isn't shown as live.
                if (live && online) item("working") {
                    // The bot as the working line reads it, with the thread's own activity and time.
                    val working = bot.copy(status = if (needsYou) "needsInput" else "working", activity = task.activity, startedAt = task.startedAt)
                    WorkingIndicator(working, workingSteps(all), details)
                }
            }
            }
    }
    StopWorkDialog(confirming, { confirming = false }) { store.stopWork(bot.id, taskId) }
    CodyncSheet(peer, { peer = null }) { BotConversationView(bot, it, state, store) }
    CodyncSheet(app, { app = null; setupEntry = null }) { ComposioConnectSheet(it, state, store, setupEntry) { store.refreshPlugins() } }
    CodyncSheet(connector, { connector = null; setupEntry = null }) { InstallConnectorSheet(it, state, store, setupEntry) }
}

/**
 * Under the thread: what its agent runs in the background, then the chat's composer writing to it ([talks]), with the
 * agent it runs on and where it works; else, for a routine's run or another bot's request, why it is read only, and Stop
 * while it runs. [stop] asks first.
 */
@Composable private fun ThreadFooter(bot: Bot, task: WorkTask, state: AppState, store: AppStore, talks: Boolean, queued: List<Entry>,
    online: Boolean, stop: () -> Unit) = Column {
    BackgroundCapsule(task)
    ThreadBox(bot, task, state, store, talks, queued, online, stop)
}

@Composable private fun ThreadBox(bot: Bot, task: WorkTask, state: AppState, store: AppStore, talks: Boolean, queued: List<Entry>,
    online: Boolean, stop: () -> Unit) {
    val live = task.status.live
    val readOnly = readOnlyLine(task)
    val draftKey = threadDraftKey(bot.id, task.id)
    if (talks) ThreadComposer(state, store, draftKey, task.talkFiles, queued, steer = { store.steerThread(bot.id, task.id, it) },
        remove = { store.unqueueThread(bot.id, task.id, it) }, working = live, stop = stop,
        accessory = { Box(Modifier.weight(1f, fill = false)) { ThreadRunsOn(bot, task, state) } }) { message, clear -> store.sendToThread(bot.id, task.id, draftKey, message); clear() }
    else if (readOnly != null) Row(Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally)) {
        Text(tr(readOnly), Modifier.weight(1f, fill = false), style = Type.footnote, color = Palette.tertiary, textAlign = TextAlign.Center)
        // It can't be written to, but a run under way can still be stopped.
        if (live) IconButton("Stop", "stop.fill", enabled = online && workKey(bot.id) !in state.busy, action = stop)
    }
}

/**
 * One of the thread's messages: what it was told first ([toldId]) folded, what the host told it later as a quote, its
 * own messages and replies as the chat's plain bubbles; what it put in the bot's chat is that chat's entry, answered and
 * opened here as there.
 */
@Composable private fun ThreadMessage(item: ChatItem.Message, toldId: String?, bot: Bot, state: AppState, store: AppStore,
    connect: ConnectionRequestActions, addFallback: ((Bot) -> Unit)?, openArtifact: (OpenArtifact) -> Unit) {
    val entry = item.entry
    val own = entry.botId != bot.id
    when {
        own && entry.id == toldId -> Told(entry.text)
        // What the host told it later (a new brief) reads as a quote, not as something the user typed.
        own && entry.isToldToThread -> SelectionContainer {
            Text("“${entry.text}”", Modifier.padding(top = 10.dp, start = 4.dp, end = 4.dp), style = Type.subheadline, color = Palette.secondary)
        }
        own && (entry.kind == "user" || entry.kind == "agent") -> Box(Modifier.padding(top = if (item.groupStart) 12.dp else 4.dp)) {
            PlainMessageRow(entry, state, store)
        }
        else -> ChatRow(entry, item.groupStart, bot, state, store, openTrace = null, reply = null, selectText = null,
            connect = connect, addFallback = addFallback, openArtifact = openArtifact)
    }
}

/**
 * A thread's rows in the chat's form (the desktop's `buildChat`): what it was told first, then its messages with what it
 * ([threadId]) put in the bot's [chat] where it came, and the user's [sends] on their way. Its steps are no rows (they
 * show in the working line), nor, while it is [live], the user's messages that wait above the composer.
 */
internal fun threadChatItems(entries: List<Entry>, chat: List<Entry>, threadId: String?, sends: List<PendingSend>, live: Boolean): List<ChatItem> {
    val thread = threadTranscript(entries)
    val own = listOfNotNull(thread.told) + thread.hidingQueue(live)
    val shown = if (threadId == null) own else sharing(own, chat.filterNot { it.isRemoved }, threadId)
    // A send whose message arrived shows as that message, wherever it is (waiting above the composer too).
    return chatItems(shown.filter { it.isChat }, sends.filter { send -> entries.none { it.clientNonce == send.nonce } })
}

/**
 * Back, the bot (a thread without a bot: the agent it runs on) and the task's title over one quiet line (its status and
 * how long its thread has run, then where it works when the composer's row doesn't say: [showsWhere]), and Details
 * ([details]) when it opens from here.
 */
@Composable private fun ThreadHeader(bot: Bot, task: WorkTask, state: AppState, showsWhere: Boolean, back: () -> Unit, details: ((DetailsRequest) -> Unit)?,
    modifier: Modifier) {
    // The Bots tab's header: glass Back and Details, the title between them.
    Row(modifier.fillMaxWidth().padding(top = 6.dp).padding(horizontal = 16.dp).heightIn(min = 44.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        GlassBackButton(back)
        Column(Modifier.weight(1f), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(7.dp)) {
                if (bot.plain) AgentIcon(agentRegistry(state, task.agentOf(bot) ?: bot.backend), 18.dp) else BotAvatar(bot, state.bots, 22.dp, animated = false)
                Text(task.title, Modifier.semantics { heading() }, style = Type.system(13f).semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Row(Modifier.testTag("thread-status").semantics(mergeDescendants = true) {}, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    // The word beside it says the same.
                    Box(Modifier.clearAndSetSemantics {}) { WorkMark(task.status) }
                    Text(tr(task.status.title), style = Type.footnote, color = Palette.secondary, maxLines = 1)
                    WorkElapsed(task)
                }
                if (showsWhere) whereLine(task, state.home)?.let { WherePath(it, Modifier.weight(1f, fill = false)) }
            }
        }
        // Kept as wide when there are none, so the title stays centered.
        if (details != null) HeaderButton("sidebar.right", "Details") { details(DetailsRequest()) } else Spacer(Modifier.size(44.dp))
    }
}

/**
 * What the thread runs on, in its composer's row: the agent, then where it works unless that is its bot's own folder,
 * which goes without saying (the desktop's).
 */
@Composable private fun ThreadRunsOn(bot: Bot, task: WorkTask, state: AppState) {
    val agent = task.agentOf(bot) ?: bot.backend
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.semantics(mergeDescendants = true) {}, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            AgentIcon(agentRegistry(state, agent), 14.dp)
            Text(backendName(state, agent), style = Type.footnote, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        val ownFolder = bot.managedWorkspace && task.worktree == null && task.folder == bot.cwd
        if (!ownFolder) whereLine(task, state.home)?.let { WherePath(it, Modifier.weight(1f, fill = false)) }
    }
}

/** Where the thread works, as its row says it; a tap copies the full path and says so for a moment. */
@Composable private fun WherePath(where: WhereLine, modifier: Modifier) {
    val path = where.path ?: return WhereLabel(where, modifier)
    val (copied, copy) = rememberCopyPath()
    WhereLabel(where, modifier.testTag("thread-where").clickable(role = Role.Button, onClickLabel = tr("Copy path")) { copy(path) }.semantics { liveRegion = LiveRegionMode.Polite }, text = if (copied) tr("Path copied") else where.text)
}

/** What the host told the thread first (the task's prompt), folded on top of its thread. */
@Composable private fun Told(told: String) {
    var expanded by rememberSaveable { mutableStateOf(false) }
    Disclosure(expanded, { expanded = !expanded }, Modifier.fillMaxWidth().padding(top = 6.dp), label = {
        Text(tr("What it was told"), style = Type.footnote.medium, color = Palette.secondary)
    }) {
        SelectionContainer {
            Text(told, Modifier.fillMaxWidth().clip(RoundedRectangle(12.dp)).background(Palette.bubbleAgent).padding(12.dp),
                style = Type.footnote, color = Palette.secondary)
        }
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
