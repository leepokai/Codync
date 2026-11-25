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
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
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

/** A bot in the Threads list, with its computer's state. */
internal class WorkingBot(val state: AppState, val bot: Bot)

/**
 * The bots of the computers the roster shows ([shown], in its order), each computer's in the order its roster shows them
 * (pinned, then section by section as [sectionOrder] has that computer's), a folded section's or computer's included; then
 * each of those computers' box of threads started without a bot, which the roster hides.
 */
internal fun threadRoster(shown: List<AppState>, sectionOrder: (AppState) -> List<String>): List<WorkingBot> =
    shown.flatMap { state -> shownBots(roster(state.bots), sectionOrder(state)).map { WorkingBot(state, it) } } +
        shown.mapNotNull { state -> state.bots.firstOrNull { it.plain && !it.deleted }?.let { WorkingBot(state, it) } }

/** How many of [roster]'s tasks a thread works on now: the tab's badge. */
internal fun workingCount(roster: List<WorkingBot>): Int = roster.sumOf { of -> of.bot.work.count { it.status == WorkStatus.Working } }

/** The rows of Recent shown before "Show more" (desktop `SHOWN_RECENT`). */
private const val SHOWN_RECENT = 8

/** A thread of a computer's box without a bot, open over the Threads tab: that box has no chat or details to open it in. */
internal data class LooseThread(val computerId: String, val botId: String, val taskId: String)

/**
 * The Threads tab, the desktop's Threads list: every task of [roster]'s bots, what needs the user first, then the groups the
 * list's options make (folders and routines' runs, a group per bot, or one per computer), then the rest under Recent,
 * newest first, the last of it on demand. A row is a thread's title and one mark for how it stands; a group's heading folds
 * it (on this device). Tapping a row that needs the user opens the chat at its card, one a thread has or had opens the
 * bot's details at its thread, any other the bot's details; touching and holding it offers what its bot's details do.
 * The box of threads without a bot has no chat or details: its rows open their thread over the list ([loose], which
 * [openLoose] sets and clears), and one without a thread opens nothing (the desktop's).
 */
@Composable internal fun ThreadsView(roster: List<WorkingBot>, filter: ComputerFilter, accounts: AccountStore, bottomInset: Dp, loose: LooseThread?,
    openLoose: (LooseThread?) -> Unit) {
    NavStack(listOfNotNull<Any>(ThreadsPage, loose), key = { (it as? LooseThread)?.let { l -> "${l.computerId}/${l.botId}/${l.taskId}" } ?: "threads" }, pop = { openLoose(null) }) { page ->
        if (page is LooseThread) LooseThreadPage(page, roster, accounts) { openLoose(null) }
        else ThreadsList(roster, filter, accounts, bottomInset, openLoose)
    }
}

private data object ThreadsPage

@Composable private fun LooseThreadPage(ref: LooseThread, roster: List<WorkingBot>, accounts: AccountStore, back: () -> Unit) {
    val of = roster.firstOrNull { it.state.computer.id == ref.computerId && it.bot.id == ref.botId }
    val store = accounts.store(ref.computerId)
    // Its computer or box gone meanwhile: nothing to show.
    LaunchedEffect(of == null || store == null) { if (of == null || store == null) back() }
    if (of == null || store == null) return
    Box(Modifier.fillMaxSize().background(Palette.background).statusBarsPadding().navigationBarsPadding().imePadding()) {
        // That box has no chat for what a thread put there.
        WorkThreadView(of.bot, ref.taskId, of.state, store, openInChat = {}, back = back)
    }
}

@Composable private fun ThreadsList(roster: List<WorkingBot>, filter: ComputerFilter, accounts: AccountStore, bottomInset: Dp, openLoose: (LooseThread) -> Unit) {
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
    // The New thread sheet is open.
    var starting by remember { mutableStateOf(false) }
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
    val bots = threadBots(roster)
    val shown = ComputerSelection(filter.states.map { it.computer.id }, filter.hidden).shown.mapNotNull { id -> filter.states.firstOrNull { it.computer.id == id } }
    val plain = plainComputer(shown)
    val line: @Composable (ThreadRow<WorkingBot>, String) -> Unit = { row, label -> ThreadLine(row, label, accounts, { question = it }, openLoose) }
    Column(Modifier.fillMaxSize().background(Palette.background).statusBarsPadding()) {
        Row(Modifier.fillMaxWidth().padding(top = 6.dp).padding(horizontal = 16.dp).height(44.dp), verticalAlignment = Alignment.CenterVertically) {
            if (searching) {
                ListSearchField(query, { query = it }, "Search") { searching = false; query = "" }
                return@Row
            }
            // The title at the start, as the Bots tab's sits beside its account button, so the buttons never cover it.
            Text(tr("Threads"), Modifier.weight(1f).semantics { heading() }, style = Type.body.semibold, color = Palette.text, maxLines = 1)
            // The desktop list header's buttons, as on the Bots tab: the computers to show, Search and New thread.
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                ComputerFilterHeader(filter.states, filter.hidden, filter.setHidden, filter.reconnect, filter.manage, options)
                HeaderButton("magnifyingglass", "Search") { searching = true }
                if (bots.isNotEmpty() || plain != null) HeaderButton("plus", "New thread") { starting = true }
            }
        }
        // An empty search keeps the list, as on the iPhone.
        if (searching && query.isNotBlank()) {
            ThreadSearchResults(threadMatches(roster, { it.bot }, query), bottomInset, line)
            return@Column
        }
        if (roster.none { it.bot.work.isNotEmpty() }) {
            EmptyState("checklist", "No threads right now", Modifier.fillMaxSize(), message = "Tasks your bots work on show up here.")
            return@Column
        }
        Column(Modifier.verticalScroll(rememberScrollState()).padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = bottomInset),
            verticalArrangement = Arrangement.spacedBy(16.dp)) {
            if (sections.needsYou.isNotEmpty()) ThreadSection("Needs you") { for (row in sections.needsYou) line(row, row.task.title) }
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
                for (row in shown) line(row, row.task.title)
                if (sections.recent.size > shown.size) Text(tr("Show more"), Modifier.fillMaxWidth().plainButton { more = true }.padding(vertical = 10.dp),
                    style = Type.body, color = Palette.tertiary)
            }
        }
    }
    WorkQuestionDialogs(question) { question = null }
    NewThreadSheet(starting, bots, plain, accounts, { starting = false }, openLoose)
}

/** Search's threads, flat and newest first, as the list draws its rows. */
@Composable private fun ThreadSearchResults(rows: List<ThreadRow<WorkingBot>>, bottomInset: Dp, line: @Composable (ThreadRow<WorkingBot>, String) -> Unit) {
    if (rows.isEmpty()) {
        EmptyState("magnifyingglass", "No results", Modifier.fillMaxSize())
        return
    }
    Column(Modifier.verticalScroll(rememberScrollState()).padding(start = 20.dp, end = 20.dp, top = 8.dp, bottom = bottomInset)) {
        for (row in rows) line(row, row.task.title)
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
@Composable private fun ThreadGroupRows(group: ThreadGroup<WorkingBot>, folded: Boolean, line: @Composable (ThreadRow<WorkingBot>, String) -> Unit,
    toggle: () -> Unit) {
    val turn by animateFloatAsState(if (folded) -90f else 0f, Motion.layout(), label = "Chevron")
    val first = group.rows.first().of
    // The box of threads without a bot is no bot: its group says so.
    val plain = group.kind == ThreadGroupKind.Bot && first.bot.plain
    val loud = if (folded) (group.rows.firstOrNull { it.task.status == WorkStatus.Failed } ?: group.rows.firstOrNull { it.task.status == WorkStatus.Working }) else null
    Row(Modifier.fillMaxWidth().plainButton(action = toggle).padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        when (group.kind) {
            ThreadGroupKind.Bot -> if (plain) Symbol("person.crop.circle.dashed", size = 14.dp, tint = Palette.secondary)
                else BotAvatar(first.bot, first.state.bots, 18.dp, animated = false)
            ThreadGroupKind.Folder -> Symbol("folder", size = 14.dp, tint = Palette.secondary)
            ThreadGroupKind.Routine -> Symbol("clock", size = 14.dp, tint = Palette.secondary)
            ThreadGroupKind.Computer -> Symbol("desktopcomputer", size = 14.dp, tint = Palette.secondary)
        }
        Text(if (plain) tr("No bot") else group.title, Modifier.weight(1f), style = Type.subheadline.semibold, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        loud?.let { WorkMark(it.task.status) }
        Symbol("chevron.down", Modifier.rotate(turn), size = 12.dp, tint = Palette.tertiary)
    }
    if (folded) return
    val translate = translator()
    for (row in group.rows) line(row,
        if (group.kind == ThreadGroupKind.Routine) RelativeTime.separator(row.task.createdAt, translate("Today"), translate("Yesterday")) else row.task.title)
}

/**
 * One thread: its title ([label]) and, unless it's done, its mark. Stop and Cut in [ask] first; a thread of the box without
 * a bot opens over the list ([openLoose]).
 */
@Composable private fun ThreadLine(row: ThreadRow<WorkingBot>, label: String, accounts: AccountStore, ask: (WorkQuestion) -> Unit, openLoose: (LooseThread) -> Unit) {
    val (of, task) = row
    val store = accounts.store(of.state.computer.id) ?: return
    val bot = of.bot
    val context = LocalContext.current
    val enabled = of.state.actualConnection is LinkState.Ready && workKey(bot.id) !in of.state.busy
    val showThread: () -> Unit = {
        if (task.thread != null) {
            if (bot.plain) openLoose(LooseThread(of.state.computer.id, bot.id, task.id)) else store.openDetails(bot.id, task.id)
        }
    }
    val open = {
        when {
            bot.plain -> showThread()
            task.status == WorkStatus.NeedsYou -> store.openCard(bot.id, task)
            task.thread != null -> showThread()
            else -> store.openDetails(bot.id)
        }
    }
    val openChat = { if (task.status == WorkStatus.NeedsYou) store.openCard(bot.id, task) else store.openBot(bot.id) }
    Row(Modifier.fillMaxWidth().heightIn(min = 40.dp).contextActions(onClick = open) {
        threadMenu(task, bot, enabled, showThread, openChat, run = { store.runWork(it, task, bot, ask) }) { text ->
            context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText("Codync", text))
        }
    }, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
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
