package com.codync.android

import android.content.ClipData
import android.content.ClipboardManager
import androidx.compose.animation.core.animateFloatAsState
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
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.snapshotFlow
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.ComputerSelection
import com.codync.android.core.LinkState
import com.codync.android.core.OWN_FOLDER_GROUP
import com.codync.android.core.ThreadGroup
import com.codync.android.core.ThreadGroupKind
import com.codync.android.core.ThreadGrouping
import com.codync.android.core.ThreadRow
import com.codync.android.core.WorkAction
import com.codync.android.core.WorkStatus
import com.codync.android.core.WorkTask
import com.codync.android.core.threadMatches
import com.codync.android.core.threadSections
import com.codync.android.core.wherePath
import com.codync.android.core.workActions
import com.codync.android.design.*
import kotlinx.coroutines.flow.distinctUntilChanged

/** A bot in the Threads list, with its computer's state. */
internal class WorkingBot(val state: AppState, val bot: Bot)

/**
 * The bots of the computers the roster shows ([shown], in its order), each computer's in the order its roster shows them
 * (pinned, then section by section as [sectionOrder] has that computer's), a folded section's or computer's included; then
 * each of those computers' box of threads started without a bot, which the roster hides. Each bot's `work` holds its
 * computer's older threads it doesn't ([withOlder]), so the list, search and a thread's page treat them as its own.
 */
internal fun threadRoster(shown: List<AppState>, sectionOrder: (AppState) -> List<String>): List<WorkingBot> =
    shown.flatMap { state -> shownBots(roster(state.bots), sectionOrder(state)).map { WorkingBot(state, state.withOlder(it)) } } +
        shown.mapNotNull { state -> state.bots.firstOrNull { it.plain && !it.deleted }?.let { WorkingBot(state, state.withOlder(it)) } }

/** How many of [roster]'s tasks a thread works on now: the tab's badge. */
internal fun workingCount(roster: List<WorkingBot>): Int = roster.sumOf { of -> of.bot.work.count { it.status == WorkStatus.Working } }

/** The rows of Recent shown before "Show more" (desktop `SHOWN_RECENT`). */
private const val SHOWN_RECENT = 8

/** A page pushed over the Threads list. */
internal sealed interface ThreadsRoute

/** A thread open over the Threads tab: [computerId]'s bot [botId]'s (or its box without a bot's) task [taskId]. */
internal data class ThreadRef(val computerId: String, val botId: String, val taskId: String) : ThreadsRoute

/** The page a new thread starts from, which its thread chat replaces once sent. */
internal data object NewThreadRoute : ThreadsRoute

/**
 * The Threads tab, the desktop's Threads list: every task of [roster]'s bots, what needs the user first, then the groups the
 * list's options make (folders and routines' runs, a group per bot, or one per computer), then the rest under Recent,
 * newest first, the last of it on demand. A row is a thread's title and one mark for how it stands; a group's heading folds
 * it (on this device). Tapping a row a thread has or had pushes its thread chat over the list ([open], which [openThread]
 * sets and clears); one with no thread that needs the user opens the chat at its card, any other the bot's details (the
 * box of threads without a bot: nothing, the desktop's). Touching and holding a row offers what its bot's details do.
 * The header's + pushes the page a new thread starts from, which the thread's chat replaces once it starts.
 */
@Composable internal fun ThreadsView(app: AccountContext, account: () -> Unit, roster: List<WorkingBot>, filter: ComputerFilter, accounts: AccountStore,
    bottomInset: Dp, open: ThreadsRoute?, active: Boolean, openThread: (ThreadsRoute?) -> Unit) {
    val bots = threadBots(roster)
    val shown = ComputerSelection(filter.states.map { it.computer.id }, filter.hidden).shown.mapNotNull { id -> filter.states.firstOrNull { it.computer.id == id } }
    val plain = plainComputer(shown)
    // A thread that starts once its page was left opens nowhere.
    val current by rememberUpdatedState(open)
    NavStack(listOfNotNull<Any>(ThreadsPage, open), key = {
        when (it) {
            is ThreadRef -> "${it.computerId}/${it.botId}/${it.taskId}"
            NewThreadRoute -> "new"
            else -> "threads"
        }
    }, pop = { openThread(null) }) { page ->
        when (page) {
            is ThreadRef -> ThreadPage(page, roster, accounts) { openThread(null) }
            NewThreadRoute -> NewThreadPage(bots, plain, accounts, back = { openThread(null) }) { if (current == NewThreadRoute) openThread(it) }
            else -> ThreadsList(app, account, roster, filter, accounts, bottomInset, active, shown, bots.isNotEmpty() || plain != null, openThread)
        }
    }
}

private data object ThreadsPage

/** A thread chat pushed over the list, with its bot's details over it on demand (the desktop's panel). */
@Composable private fun ThreadPage(ref: ThreadRef, roster: List<WorkingBot>, accounts: AccountStore, back: () -> Unit) {
    val of = roster.firstOrNull { it.state.computer.id == ref.computerId && it.bot.id == ref.botId }
    val store = accounts.store(ref.computerId)
    // Its computer or bot gone meanwhile: nothing to show.
    LaunchedEffect(of == null || store == null) { if (of == null || store == null) back() }
    if (of == null || store == null) return
    var details by remember(ref) { mutableStateOf<DetailsRequest?>(null) }
    var viewing by remember(ref) { mutableStateOf<OpenArtifact?>(null) }
    // The box of threads without a bot has no details.
    ThreadChat(of.bot, ref.taskId, of.state, store, inDetails = false, openDetails = if (of.bot.plain) null else { request -> details = request },
        openArtifact = { viewing = it }, back = back)
    CodyncSheet(details, { details = null }) { request ->
        BotDetailsView(of.bot, request, of.state, store, openCard = null) { viewing = it }
    }
    ArtifactLayer(viewing) { open -> ArtifactViewer(open, of.bot, of.state, store) { viewing = null } }
}

@Composable private fun ThreadsList(app: AccountContext, account: () -> Unit, roster: List<WorkingBot>, filter: ComputerFilter, accounts: AccountStore, bottomInset: Dp, active: Boolean,
    shown: List<AppState>, startsThreads: Boolean, openThread: (ThreadsRoute) -> Unit) {
    val context = LocalContext.current
    var grouping by remember { mutableStateOf(ThreadsLayout.grouping(context)) }
    var byStatus by remember { mutableStateOf(ThreadsLayout.byStatus(context)) }
    var byActivity by remember { mutableStateOf(ThreadsLayout.byActivity(context)) }
    var folded by remember { mutableStateOf(ThreadsLayout.folded(context)) }
    var more by remember { mutableStateOf(false) }
    var question by remember { mutableStateOf<WorkQuestion?>(null) }
    // The search field is open, and what it finds threads by.
    var searching by remember { mutableStateOf(false) }
    var query by remember { mutableStateOf("") }
    fun track(option: LayoutOption, on: Boolean) = Analytics.capture(AnalyticsEvent.ListLayoutChanged(LayoutList.Threads, option, on))
    fun group(by: ThreadGrouping, option: LayoutOption) = layoutOption(groupTitle(by), grouping == by) { on ->
        track(option, on)
        grouping = if (on) by else ThreadGrouping.None
        ThreadsLayout.setGrouping(context, grouping)
    }
    val options = listOf(
        group(ThreadGrouping.Folder, LayoutOption.Folder), group(ThreadGrouping.Bot, LayoutOption.Bot), group(ThreadGrouping.Computer, LayoutOption.Computer),
        layoutOption("Sort by status", byStatus, divider = true) { track(LayoutOption.Status, it); byStatus = it; ThreadsLayout.setByStatus(context, it) },
        layoutOption("Sort by last active", byActivity) { track(LayoutOption.Activity, it); byActivity = it; ThreadsLayout.setByActivity(context, it) },
    )
    val sections = threadSections(roster, { it.bot }, { "${it.state.computer.id}/${it.bot.id}" }, { it.state.hostName }, grouping, byStatus, byActivity)
    val line: @Composable (ThreadRow<WorkingBot>, String, Dp) -> Unit = { row, label, indent -> ThreadLine(row, label, indent, accounts, { question = it }, openThread) }
    val scroll = rememberScrollState()
    OlderThreadsPaging(roster, accounts, active, scroll, more)
    // The Bots tab's stage: the list scrolls under the glass header, softly blurred at the top.
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    GlassStage(Modifier.fillMaxSize().background(Palette.background), chrome = {
        ScrollEdge(Edge.Top, top + 64.dp)
        // The Bots tab's header: the title over how the shown computers are connected, then the computers to show, Search and New thread.
        val summary = connectionSummary(shown)
        ListHeader(app, account, "Threads", summary.filled(tr(summary.template)), searching, query, { query = it }, { searching = false; query = "" },
            Modifier.statusBarsPadding()) {
            ComputerFilterHeader(filter.states, filter.hidden, filter.setHidden, filter.reconnect, filter.manage, options)
            HeaderButton("magnifyingglass", "Search") { searching = true }
            if (startsThreads) HeaderButton("plus", "New thread") { openThread(NewThreadRoute) }
        }
    }) {
        // An empty search keeps the list, as on the iPhone.
        if (searching && query.isNotBlank()) {
            ThreadSearchResults(threadMatches(roster, { it.bot }, query), top + 56.dp, bottomInset, line)
            return@GlassStage
        }
        if (roster.none { it.bot.work.isNotEmpty() }) {
            EmptyState("checklist", "No threads right now", Modifier.fillMaxSize(), message = "Tasks your bots work on show up here.")
            return@GlassStage
        }
        Column(Modifier.fillMaxSize().verticalScroll(scroll).padding(start = 20.dp, end = 20.dp, top = top + 64.dp, bottom = bottomInset),
            verticalArrangement = Arrangement.spacedBy(16.dp)) {
            if (sections.needsYou.isNotEmpty()) ThreadSection("Needs you") { for (row in sections.needsYou) line(row, row.task.title, 0.dp) }
            for (kind in ThreadGroupKind.entries) {
                val mine = sections.groups.filter { it.kind == kind }
                if (mine.isNotEmpty()) ThreadSection(kindTitle(kind)) {
                    for (group in mine) ThreadGroupRows(group, group.key in folded, line) {
                        folded = if (group.key in folded) folded - group.key else folded + group.key
                        ThreadsLayout.setFolded(context, folded)
                    }
                }
            }
            if (sections.recent.isNotEmpty()) ThreadSection("Recent") {
                val shown = if (more) sections.recent else sections.recent.take(SHOWN_RECENT)
                for (row in shown) line(row, row.task.title, 0.dp)
                if (sections.recent.size > shown.size) Text(tr("Show more"), Modifier.fillMaxWidth().plainButton { more = true }.padding(vertical = 10.dp),
                    style = Type.body, color = Palette.tertiary)
            }
        }
    }
    WorkQuestionDialogs(question) { question = null }
}

/**
 * Reads the older threads of [roster]'s computers while the list shows ([active]): each one's first page once its link
 * is up, then the next page of every one with more each time the list scrolls to its end (or "Show more" shows the rest).
 */
@Composable private fun OlderThreadsPaging(roster: List<WorkingBot>, accounts: AccountStore, active: Boolean, scroll: ScrollState, showingAll: Boolean) {
    val states = roster.map { it.state }.distinctBy { it.computer.id }
    val unread = if (active) states.filter { it.olderThreads == null && it.actualConnection is LinkState.Ready && threadKeepDays(it.hello) != null }
        .map { it.computer.id } else emptyList()
    LaunchedEffect(unread) { unread.forEach { accounts.store(it)?.loadOlderThreads() } }
    val paged by rememberUpdatedState(states.filter { it.olderThreads?.more == true }.map { it.computer.id })
    LaunchedEffect(active, scroll, showingAll) {
        if (!active) return@LaunchedEffect
        snapshotFlow { !scroll.canScrollForward }.distinctUntilChanged().collect { atEnd ->
            if (atEnd) paged.forEach { accounts.store(it)?.loadOlderThreads() }
        }
    }
}

/** Search's threads, flat and newest first, as the list draws its rows. */
@Composable private fun ThreadSearchResults(rows: List<ThreadRow<WorkingBot>>, topInset: Dp, bottomInset: Dp, line: @Composable (ThreadRow<WorkingBot>, String, Dp) -> Unit) {
    if (rows.isEmpty()) {
        EmptyState("magnifyingglass", "No results", Modifier.fillMaxSize())
        return
    }
    Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(start = 20.dp, end = 20.dp, top = topInset + 8.dp, bottom = bottomInset)) {
        for (row in rows) line(row, row.task.title, 0.dp)
    }
}

private fun groupTitle(by: ThreadGrouping) = when (by) {
    ThreadGrouping.Bot -> "Group by bot"
    ThreadGrouping.Computer -> "Group by computer"
    else -> "Group by folder"
}

private fun kindTitle(kind: ThreadGroupKind) = when (kind) {
    ThreadGroupKind.Folder -> "Folders"
    ThreadGroupKind.Routine -> "Routines"
    ThreadGroupKind.Bot -> "Bots"
    ThreadGroupKind.Computer -> "Computers"
}

@Composable private fun ThreadSection(title: String, rows: @Composable () -> Unit) {
    Column {
        Text(tr(title), Modifier.padding(bottom = 4.dp).semantics { heading() }, style = Type.caption, color = Palette.tertiary)
        rows()
    }
}

/** A group's heading, which folds its rows; folded, it still marks a row that failed or works. A routine's runs read by when they started. */
@Composable private fun ThreadGroupRows(group: ThreadGroup<WorkingBot>, folded: Boolean, line: @Composable (ThreadRow<WorkingBot>, String, Dp) -> Unit,
    toggle: () -> Unit) {
    val turn by animateFloatAsState(if (folded) -90f else 0f, Motion.layout(), label = "Chevron")
    val first = group.rows.first().of
    // The box of threads without a bot is no bot: its group says so.
    val plain = group.kind == ThreadGroupKind.Bot && first.bot.plain
    val loud = if (folded) (group.rows.firstOrNull { it.task.status == WorkStatus.Failed } ?: group.rows.firstOrNull { it.task.status == WorkStatus.Working }) else null
    Row(Modifier.fillMaxWidth().plainButton(action = toggle).padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        when (group.kind) {
            ThreadGroupKind.Bot -> if (plain) Symbol("app.dashed", size = 18.dp, tint = Palette.secondary)
                else BotAvatar(first.bot, first.state.bots, 18.dp, animated = false)
            ThreadGroupKind.Folder -> Symbol("folder", size = 14.dp, tint = Palette.secondary)
            ThreadGroupKind.Routine -> Symbol("clock", size = 14.dp, tint = Palette.secondary)
            ThreadGroupKind.Computer -> Symbol("desktopcomputer", size = 14.dp, tint = Palette.secondary)
        }
        Text(if (plain) tr("No bot") else if (group.key == OWN_FOLDER_GROUP) tr(group.title) else group.title, Modifier.weight(1f), style = Type.subheadline.semibold, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        loud?.let { WorkMark(it.task.status) }
        Symbol("chevron.down", Modifier.rotate(turn), size = 12.dp, tint = Palette.tertiary)
    }
    if (folded) return
    val translate = translator()
    for (row in group.rows) line(row,
        if (group.kind == ThreadGroupKind.Routine) RelativeTime.separator(row.task.createdAt, translate("Today"), translate("Yesterday")) else row.task.title,
        26.dp)
}

/**
 * One thread: its title ([label], [indent]ed under a group's heading) and, unless it's done, its mark. Stop and Cut in [ask] first; its thread chat opens over
 * the list ([openThread]).
 */
@Composable private fun ThreadLine(row: ThreadRow<WorkingBot>, label: String, indent: Dp, accounts: AccountStore, ask: (WorkQuestion) -> Unit, openThread: (ThreadRef) -> Unit) {
    val (of, task) = row
    val store = accounts.store(of.state.computer.id) ?: return
    val bot = of.bot
    val context = LocalContext.current
    val enabled = of.state.actualConnection is LinkState.Ready && workKey(bot.id) !in of.state.busy
    val showThread: () -> Unit = { if (task.thread != null) openThread(ThreadRef(of.state.computer.id, bot.id, task.id)) }
    val open = {
        when {
            task.thread != null -> showThread()
            bot.plain -> {}
            task.status == WorkStatus.NeedsYou -> store.openCard(bot.id, task)
            else -> store.openDetails(bot.id)
        }
    }
    val openChat = { if (task.status == WorkStatus.NeedsYou) store.openCard(bot.id, task) else store.openBot(bot.id) }
    Row(Modifier.fillMaxWidth().heightIn(min = 40.dp).contextActions(onClick = open) {
        threadMenu(task, bot, enabled, showThread, openChat, run = { store.runWork(it, task, bot, ask) }) { text ->
            context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Codync", text))
        }
    }.padding(start = indent), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        // The coding agent it runs on; its place kept without one, so titles line up.
        Box(Modifier.size(16.dp)) { task.agentOf(bot)?.let { AgentIcon(agentRegistry(of.state, it), 16.dp) } }
        Text(label, Modifier.weight(1f), style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        if (task.status != WorkStatus.Done) WorkMark(task.status)
    }
}

/**
 * A thread's touch-and-hold menu (the desktop's `threadMenuItems` as far as the phone goes, iOS `ThreadList.menu`): where
 * it goes, what its bot's box does with it, what can be copied, and Drop last.
 */
private fun threadMenu(task: WorkTask, bot: Bot, enabled: Boolean, showThread: () -> Unit, openChat: () -> Unit, run: (WorkAction) -> Unit,
    copy: (String) -> Unit): List<MenuItem> {
    val go = listOfNotNull(
        MenuItem("Open thread", icon = "arrow.triangle.branch") { showThread() }.takeIf { task.thread != null },
        // The box of threads without a bot has no chat of its own.
        MenuItem(if (task.status == WorkStatus.NeedsYou) "Answer in chat" else "Open chat", icon = "bubble.left") { openChat() }.takeIf { !bot.plain },
    )
    val actions = workActions(task, bot.work, bot.maxThreads)
    val inBox = actions.filter { it != WorkAction.Drop }.mapIndexed { index, action ->
        MenuItem(action.title, icon = action.symbol, destructive = action.destructive, divider = index == 0, enabled = enabled) { run(action) }
    }
    val copies = listOfNotNull(MenuItem("Copy title", icon = "doc.on.doc", divider = true) { copy(task.title) },
        task.wherePath?.let { MenuItem("Copy folder path", icon = "folder") { copy(it) } })
    // Drop takes it out of the box for good: last, apart, in red.
    val drop = listOfNotNull(MenuItem("Drop", icon = "xmark", destructive = true, divider = true, enabled = enabled) { run(WorkAction.Drop) }
        .takeIf { WorkAction.Drop in actions })
    return go + inBox + copies + drop
}
