package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyItemScope
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import android.content.Context
import androidx.compose.ui.unit.Dp
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.zIndex
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import coil3.compose.AsyncImage
import com.codync.android.core.Bot
import com.codync.android.core.ComputerSelection
import com.codync.android.core.LinkState
import com.codync.android.core.folderName
import com.codync.android.core.mixesAgents
import com.codync.android.core.runningAgent
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/**
 * One computer's roster while the list shows it: its rows' and sections' drags and its section order (kept on this
 * device), held in [orders] by [key], which the Threads tab reads too.
 */
private class ComputerRoster(private val context: Context, val key: String, private val orders: MutableMap<String, List<String>>) {
    val reorder = RosterReorder()
    val sections = ListReorder()
    var order: List<String>
        get() = orders[key].orEmpty()
        set(value) { orders[key] = value }

    init { if (key !in orders) orders[key] = SectionOrders.load(context, key) }

    /** Points the drags at [state]'s bots and sections as [list] shows them. */
    fun sync(state: AppState, list: LazyListState) {
        val bots = roster(state.bots)
        reorder.roster = bots
        sections.names = orderedSections(sectionNames(bots.filterNot { it.pinned }) +
            if (bots.any { !it.pinned && it.section == null }) listOf("") else emptyList(), order)
        sections.save = { order = it; sections.names = it; SectionOrders.save(context, key, it) }
        sections.position = { name ->
            val item = "${state.computer.id}:" + if (name.isEmpty()) "label:bots" else "section:$name"
            list.layoutInfo.visibleItemsInfo.firstOrNull { it.key == item }?.offset?.toFloat() ?: 0f
        }
    }
}

/**
 * The roster, as iOS `BotListView` draws it: every bot on every computer is a person you can message. One
 * computer shows a flat list; more get a heading each, which folds its bots and drags to reorder the computers.
 */
@Composable internal fun BotListView(app: AccountContext, states: List<AppState>, accounts: AccountStore, account: () -> Unit, manage: () -> Unit,
    newBot: (String) -> Unit, newGroup: (String) -> Unit, edit: (String, Bot) -> Unit, extensions: (String) -> Unit, bottomInset: Dp,
    hidden: String, setHidden: (String) -> Unit, sectionOrders: MutableMap<String, List<String>>) {
    var confirmDelete by remember { mutableStateOf<Pair<String, Bot>?>(null) }
    // A bot whose agent starts over with a fresh context, once confirmed.
    var confirmNewSession by remember { mutableStateOf<Pair<String, Bot>?>(null) }
    var naming by remember { mutableStateOf<Naming?>(null) }
    val context = LocalContext.current
    var folded by remember { mutableStateOf(FoldedSections.load(context)) }
    var pins by remember { mutableStateOf(0) }
    // The search field is open, and what it finds bots by.
    var searching by remember { mutableStateOf(false) }
    var query by remember { mutableStateOf("") }
    var collapsed by remember { mutableStateOf(RosterComputers.folded(context)) }
    val listState = rememberLazyListState()
    val rosters = remember(app.contextId) { mutableMapOf<String, ComputerRoster>() }
    val shownIds = ComputerSelection(states.map { it.computer.id }, hidden).shown
    val shown = states.filter { it.computer.id in shownIds }
    // The list's layout, kept on this device; several computers each get a heading unless the list is asked for as one.
    var byComputer by remember { mutableStateOf(BotListLayout.byComputer(context)) }
    var byStatus by remember { mutableStateOf(BotListLayout.byStatus(context)) }
    val several = shown.size > 1
    val grouped = several && byComputer
    // The computers shown that can move a bot with a new message to the top: theirs to keep, so it reaches every device.
    val ordering = shown.filter { it.connection is LinkState.Ready && autoOrder(it.hello) != null }
    val byActivity = ordering.isNotEmpty() && ordering.all { autoOrder(it.hello) == true }
    val layOut = { option: LayoutOption, on: Boolean ->
        Analytics.capture(AnalyticsEvent.ListLayoutChanged(LayoutList.Bots, option, on))
        when (option) {
            LayoutOption.Computer -> { byComputer = on; BotListLayout.setByComputer(context, on) }
            LayoutOption.Status -> { byStatus = on; BotListLayout.setByStatus(context, on) }
            else -> ordering.forEach { accounts.store(it.computer.id)?.setAutoOrder(on) }
        }
    }
    val options = listOfNotNull(
        layoutOption("Group by computer", byComputer) { layOut(LayoutOption.Computer, it) },
        layoutOption("Sort by status", byStatus) { layOut(LayoutOption.Status, it) },
        layoutOption("Sort by last active", byActivity) { layOut(LayoutOption.Activity, it) }.takeIf { ordering.isNotEmpty() },
    )
    // Computers that can take a new bot once they're connected, and the ones that can right now (iOS `availableStores`, `onlineStores`).
    val available = availableComputers(app, states)
    val online = available.filter { it.actualConnection is LinkState.Ready }
    val computerReorder = remember { ListReorder() }
    computerReorder.names = shown.map { it.computer.id }
    computerReorder.save = { ids -> accounts.orderComputers(ids) }
    computerReorder.position = { id -> listState.layoutInfo.visibleItemsInfo.firstOrNull { it.key == "computer:$id" }?.offset?.toFloat() ?: 0f }
    // A lifted heading folds every computer's bots into their headings while it moves.
    val movingComputer = computerReorder.dragged != null
    val ordered = computerReorder.shown.mapNotNull { id -> shown.firstOrNull { it.computer.id == id } }
    for (state in shown) rosters.getOrPut(state.computer.id) { ComputerRoster(context, SectionOrders.key(app.contextId, state.computer.id), sectionOrders) }
        .sync(state, listState)
    // A folded section renamed or removed here stays folded under its new name, at once, like its bots; a refusal puts the
    // folds back, reading and writing the stored ones, as the list may have left the screen.
    val renameSection = { roster: ComputerRoster, store: AppStore, from: String, to: String? ->
        val orderBefore = roster.order
        val orderAfter = renamedSectionOrder(roster.sections.names, from, to)
        roster.sections.save(orderAfter)
        val before = folded
        val renamed = foldsAfterRename(before, from, to)
        folded = renamed
        FoldedSections.save(context, renamed)
        store.renameSection(from, to) {
            if (SectionOrders.load(context, roster.key) == orderAfter) roster.sections.save(orderBefore)
            folded = foldsAfterRefusal(FoldedSections.load(context), renamed, before).also { FoldedSections.save(context, it) }
        }
    }
    val toggleSection = { name: String -> folded = if (name in folded) folded - name else folded + name; FoldedSections.save(context, folded) }
    val agentLogos by AppearancePreferences.agentLogos.collectAsStateWithLifecycle()
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    GlassStage(Modifier.fillMaxSize().background(Palette.background), chrome = {
        ScrollEdge(Edge.Top, top + 64.dp)
        // The title over how its computers are connected; the desktop list header's buttons: the computers to show, Search and New.
        val summary = connectionSummary(shown)
        ListHeader(app, account, "Bots", summary.filled(tr(summary.template)), searching, query, { query = it }, { searching = false; query = "" },
            Modifier.statusBarsPadding(), prompt = "Search bots") {
            ComputerFilterHeader(states, hidden, setHidden, { for (state in shown) accounts.store(state.computer.id)?.reconnect() }, manage, options)
            HeaderButton("magnifyingglass", "Search") { searching = true }
            // A new bot starts on the first computer online (its dialog can move it); a group chat gathers one computer's bots.
            NewMenuButton(enabled = available.isNotEmpty(), groupEnabled = online.isNotEmpty(),
                { (online.firstOrNull() ?: available.firstOrNull())?.let { newBot(it.computer.id) } },
                { online.firstOrNull()?.let { newGroup(it.computer.id) } })
        }
    }) {
        // Pull to refresh reconnects every computer and asks the account for its computers again (iOS `refreshable`).
        PullToRefresh({ accounts.refreshRoster() }, Modifier.fillMaxSize(), indicatorTop = top + 50.dp) {
        LazyColumn(Modifier.fillMaxSize(), state = listState, contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = top + 56.dp, bottom = bottomInset)) {
            // A newer app release (dismissible; everything still works). Computers update themselves, so a newer host gets no reminder here.
            item("appUpdate") { AppUpdateReminder(Modifier.padding(vertical = 4.dp)) }
            // One computer: its update notice leads the list. A computer waiting for an update has its card instead; its bots may not be readable yet.
            if (!grouped) for (state in shown) state.mismatch?.let { mismatch -> item("update:${state.computer.id}") {
                accounts.store(state.computer.id)?.let { store -> UpdateNeededCard(state, mismatch, store::requestHostUpdate, Modifier.padding(vertical = 8.dp)) }
            } }
            if (shown.all { roster(it.bots).isEmpty() && it.mismatch == null }) item("empty") {
                // Computers that turned this phone away count as none: the way on is in Computers.
                EmptyRoster(canCreate = available.isNotEmpty(), hasComputer = states.any { !app.isStale(it, states) },
                    create = { (online.firstOrNull() ?: available.firstOrNull())?.let { newBot(it.computer.id) } }, showComputers = manage)
            }
            val search = query.trim()
            for (state in ordered) {
                val id = state.computer.id
                val store = accounts.store(id) ?: continue
                val roster = rosters[id] ?: continue
                // Search: the matching bots of every shown computer, flat, folded ones included.
                if (grouped && search.isEmpty()) item("computer:$id") {
                    ComputerHeading(state, store, computerReorder, folded = movingComputer || id in collapsed, stale = app.isStale(state, states),
                        toggle = { collapsed = if (id in collapsed) collapsed - id else collapsed + id; RosterComputers.setFolded(context, collapsed) },
                        extensions = { extensions(id) })
                }
                if (grouped && search.isEmpty() && (movingComputer || id in collapsed)) continue
                if (state.offersWorkflows && search.isEmpty()) {
                    val pinned = pinnedWorkflows(context, state).let { if (pins < 0) emptyList() else it }
                    items(pinned, key = { "workflow:$id:${it.id}" }) { workflow ->
                        PinnedWorkflowRow(workflow, pinned = true, onOpen = { accounts.focusWorkflows(id, workflow.id) }) {
                            WorkflowPins.toggle(context, id, workflow.id)
                            pins++
                        }
                    }
                }
                botLines(state, store, roster, folded, several, byStatus, agentLogos, search, RosterActions(toggleSection,
                    rename = { name -> naming = Naming("Rename section", name) { renameSection(roster, store, name, it) } },
                    remove = { name -> renameSection(roster, store, name, null) },
                    newSection = { bot -> naming = Naming("New section", "") { store.setSection(bot.id, it) } },
                    edit = { bot -> edit(id, bot) }, newSession = { bot -> confirmNewSession = id to bot }, delete = { bot -> confirmDelete = id to bot }))
            }
        }
        }
    }
    val target = lastOf(confirmDelete)
    val deleting = target?.second
    CodyncDialog("Delete ${deleting?.name ?: "bot"}?", confirmDelete != null, { confirmDelete = null },
        message = if (deleting?.kind == "group") "Its bots and their own chats stay." else "Files it changed on your computer stay as they are.") {
        // lastOf keeps the target through the fade-out and the press (dismiss clears confirmDelete first).
        listOfNotNull(target?.let { (id, bot) ->
            DialogAction(if (bot.kind == "group") "Delete group chat" else "Delete bot and its conversation", destructive = true) {
                accounts.store(id)?.deleteBot(bot) {}
            }
        })
    }
    CodyncDialog("Start a new session?", confirmNewSession != null, { confirmNewSession = null },
        message = "The conversation stays here, but the agent starts with a fresh context.") {
        // Taken now: the dialog dismisses (clearing it) before it runs the action.
        listOfNotNull(confirmNewSession?.let { (id, bot) -> DialogAction("New session") { accounts.store(id)?.newSession(bot) } })
    }
    NameDialog(naming, "Section name", { naming = null }, ::limitSectionName, ::sectionName)
}

/** A computer's heading (and, while it needs one, its update notice) that drags to reorder the computers. */
@Composable private fun LazyItemScope.ComputerHeading(state: AppState, store: AppStore, reorder: ListReorder, folded: Boolean, stale: Boolean,
    toggle: () -> Unit, extensions: () -> Unit) {
    val id = state.computer.id
    Column(reorderable(reorder, id)) {
        // Not synced while it needs an update: "No bots yet" wouldn't be true.
        ComputerSection(state, empty = state.mismatch == null && roster(state.bots).isEmpty(), folded = folded, stale = stale, toggle = toggle,
            move = { reorder.move(id, it) }, extensions = extensions.takeIf { state.connection is LinkState.Ready && state.mismatch == null },
            drag = reorder.handle(id))
        state.mismatch?.let { UpdateNeededCard(state, it, store::requestHostUpdate, Modifier.padding(vertical = 8.dp)) }
    }
}

/** What a computer's rows and section headings ask of the roster. */
private class RosterActions(val toggleSection: (String) -> Unit, val rename: (String) -> Unit, val remove: (String) -> Unit,
    val newSection: (Bot) -> Unit, val edit: (Bot) -> Unit, val newSession: (Bot) -> Unit, val delete: (Bot) -> Unit)

/**
 * One computer's bots: pinned ones, then each section's heading with its bots (none while folded), then the bots in no section.
 * [several]: the list shows several computers' bots, so a row names its computer; [byStatus]: each group's bots stand by status.
 */
private fun LazyListScope.botLines(state: AppState, store: AppStore, roster: ComputerRoster, folded: Set<String>, several: Boolean,
    byStatus: Boolean, agentLogos: Boolean, search: String, actions: RosterActions) {
    val id = state.computer.id
    val sectionReorder = roster.sections
    val reorder = roster.reorder
    val sections = menuSections(state.bots)
    // The agent's logo marks each avatar only while the computer's bots mix agents. Not before hello: it names
    // the logos, and without it every badge would be the same generic one.
    val logos = agentLogos && state.hello != null && roster(state.bots).mixesAgents()
    val bots = roster(state.bots).let { if (byStatus) sortedByStatus(it) else it }
    val lines = if (search.isEmpty()) rosterLines(reorder.shown(bots), folded, sectionReorder.shown, sectionReorder.dragged != null)
        else shownBots(roster(state.bots), roster.order, search).map(RosterLine::Row)
    items(lines, key = { "$id:${it.key}" }) { line ->
        when (line) {
            // Folding drops the section's rows from the list; animateItem fades them out and slides the rest up.
            is RosterLine.Header -> SectionHeader(line, toggle = { actions.toggleSection(line.name) },
                actions = if (state.supportsSections) sectionActions(rename = { actions.rename(line.name) }, remove = { actions.remove(line.name) }) else emptyList(),
                modifier = reorderable(sectionReorder, line.name),
                drag = sectionReorder.handle(line.name),
                move = { sectionReorder.move(line.name, it) })
            RosterLine.Label -> SectionLabel(reorderable(sectionReorder, ""), sectionReorder.handle("")) { sectionReorder.move("", it) }
            is RosterLine.Row -> {
                val bot = line.bot
                val lifted = reorder.dragged == bot.id
                val agent = bot.runningAgent.takeIf { logos && bot.kind != "group" }?.let { AgentMark(agentRegistry(state, it), backendName(state, it)) }
                Box(Modifier.onSizeChanged { reorder.heights[bot.id] = it.height }
                    .then(if (lifted) Modifier.zIndex(1f).graphicsLayer { translationY = reorder.offset; scaleX = 1.02f; scaleY = 1.02f }
                        else Modifier.animateItem())) {
                    // Search shows only some bots, and a status sort another order than the saved one: no reordering then.
                    BotRowItem(bot, state, store, reorder.handle(bot.id) { store.reorder(it) }.takeIf { search.isEmpty() && !byStatus }, sections, several, agent, actions)
                }
            }
        }
    }
}

/**
 * Drag-to-reorder for the roster (iOS `List.onMove`): long-press a row, then move it. The order
 * shown follows the finger until the host's synced order catches up.
 */
private class RosterReorder {
    var roster: List<Bot> = emptyList()
    var order by mutableStateOf<List<String>?>(null)
    var dragged by mutableStateOf<String?>(null)
    var offset by mutableFloatStateOf(0f)
    val heights = mutableMapOf<String, Int>()
    private val handles = mutableMapOf<String, ContextDrag>()
    private val synced: List<String> get() = roster.map(Bot::id)

    fun shown(bots: List<Bot>): List<Bot> {
        val ids = order ?: return bots
        // Dropped and synced: the host's order is the one to show again.
        if (dragged == null && ids == synced) { order = null; return bots }
        return ids.mapNotNull { id -> bots.firstOrNull { it.id == id } } + bots.filter { it.id !in ids }
    }

    fun handle(id: String, save: (List<String>) -> Unit): ContextDrag = handles.getOrPut(id) {
        ContextDrag(start = { order = order ?: synced; dragged = id; offset = 0f }, move = { step -> offset += step; swap(id) }, end = {
            val dropped = order
            dragged = null; offset = 0f
            if (dropped != null && dropped != synced) save(dropped) else order = null
        })
    }

    /** Past half its height, the row takes the place of the next bot it may pass ([moved]): headers and other sections stay put. */
    private fun swap(id: String) {
        val bots = shown(roster)
        val direction = if (offset > 0) 1 else -1
        val swapped = moved(bots, id, direction) ?: return
        val height = heights[swapped[bots.indexOfFirst { it.id == id }]] ?: return
        if (kotlin.math.abs(offset) < height / 2f) return
        order = swapped
        offset -= direction * height
    }
}

/** Roster order: pinned bots, then where the user dragged them, then most recent activity. */
internal fun roster(bots: List<Bot>): List<Bot> = bots.filter { !it.deleted && !it.hidden }
    .sortedWith(compareBy<Bot> { it.band }.thenBy { it.position }.thenByDescending { it.lastAt })

/**
 * The roster order with a bot swapped with the next bot up (-1) or down (+1) that shows beside it, or null when there's none:
 * a bot stays in its band (`Bot.band`: pinned bots, the rest); pinned bots move among all the pinned, the others
 * among bots of their section (or of none), skipping the bots between.
 */
internal fun moved(roster: List<Bot>, id: String, offset: Int): List<String>? {
    val at = roster.indexOfFirst { it.id == id }
    if (at < 0) return null
    val next = generateSequence(at + offset) { it + offset }.takeWhile { it in roster.indices }
        .firstOrNull { roster[it].movesWith(roster[at]) } ?: return null
    return roster.map(Bot::id).toMutableList().apply { this[at] = roster[next].id; this[next] = id }
}

private fun Bot.movesWith(other: Bot) = band == other.band && (pinned || section == other.section)

@Composable private fun BotRowItem(bot: Bot, state: AppState, store: AppStore, drag: ContextDrag?, sections: List<String>, grouped: Boolean,
    agent: AgentMark?, actions: RosterActions) {
    Box(Modifier.fillMaxWidth().contextActions(onClick = { store.openBot(bot.id) }, reorder = drag, preview = {
        Box(Modifier.padding(horizontal = 8.dp)) { BotRow(bot, state, agent) }
    }) {
        listOfNotNull(
            MenuItem(if (bot.pinned) "Unpin" else "Pin", icon = if (bot.pinned) "pin.slash" else "pin") {
                store.command("updateBot", buildJsonObject { put("id", bot.id); put("pinned", !bot.pinned) }, "roster:" + bot.id)
            },
            if (state.supportsSections) sectionMenu(bot, sections, { store.setSection(bot.id, it) }, { actions.newSection(bot) }) else null,
            if (bot.kind == "group") MenuItem("Edit group", icon = "person.2") { actions.edit(bot) }
            else MenuItem("Edit profile", icon = "pencil") { actions.edit(bot) },
            MenuItem("Mark as read", icon = "checkmark.message") {
                store.command("markRead", buildJsonObject { put("botId", bot.id) }, "roster:" + bot.id)
            },
            // A group has no agent session of its own.
            MenuItem("New session", icon = "arrow.counterclockwise") { actions.newSession(bot) }.takeIf { bot.kind != "group" },
            MenuItem("Make lead agent", icon = "star") {
                store.command("setLead", buildJsonObject { put("botId", bot.id) }, "roster:" + bot.id)
            }.takeIf { !bot.lead && bot.kind != "group" && state.supportsLeadChoice },
            // The lead stays in the list.
            MenuItem("Hide from list", icon = "eye.slash") {
                store.command("updateBot", buildJsonObject { put("id", bot.id); put("hidden", true) }, "roster:" + bot.id)
            }.takeUnless { bot.lead },
            // An older computer keeps its lead.
            MenuItem("Delete", icon = "trash", destructive = true) { actions.delete(bot) }.takeIf { !bot.lead || state.supportsLeadChoice },
        )
    }.semantics {
        // With several computers, which one it's on.
        contentDescription = if (grouped) "${bot.name}, on ${state.hostName}" else bot.name
        // TalkBack's way to reorder (the drag needs a touch and hold, then a move).
        if (drag != null) customActions = listOf(-1 to "Move up", 1 to "Move down").map { (offset, title) ->
            CustomAccessibilityAction(title) { moved(roster(state.bots), bot.id, offset)?.let(store::reorder) != null }
        }
    }) {
        BotRow(bot, state, agent)
    }
}

/** One bot: avatar with its status dot (and [agent]'s logo, if any), name and day, then the live activity or last message. */
@Composable internal fun BotRow(bot: Bot, state: AppState, agent: AgentMark? = null) {
    Row(Modifier.fillMaxWidth().padding(vertical = 10.dp), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        AvatarWithStatus(bot, state.bots, 46.dp, agent)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    if (bot.pinned) Symbol("pin.fill", size = 13.dp, tint = Palette.tertiary)
                    Text(bot.name, Modifier.weight(1f, fill = false), style = Type.body.semibold, color = Palette.text, maxLines = 1,
                        overflow = TextOverflow.Ellipsis)
                    if (state.screen?.agentBot == bot.id) Symbol("cursorarrow.motionlines", size = 14.dp, tint = Palette.accent,
                        contentDescription = tr("Using the computer"))
                }
                Spacer(Modifier.width(8.dp))
                if (bot.lastAt > 0) Text(RelativeTime.day(bot.lastAt, yesterday = tr("Yesterday")), style = Type.subheadline,
                    color = if (bot.unread > 0) Palette.text else Palette.tertiary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Box(Modifier.weight(1f)) { Preview(bot, state) }
                if (bot.unread > 0) UnreadBadge(bot.unread, waiting = bot.waiting > 0)
            }
        }
    }
}

/** A count of unread messages in the accent capsule (a row, a folded section). [waiting]: it counts a card waiting on
 *  the user, so it takes the needs-you colour of the avatar badge (iOS `UnreadBadge`). */
@Composable internal fun UnreadBadge(count: Int, waiting: Boolean) {
    Box(Modifier.height(18.dp).widthIn(min = 18.dp).clip(Capsule()).background(if (waiting) Palette.warning else Palette.accentFill)
        .padding(horizontal = 6.dp).semantics { contentDescription = "$count unread" }, contentAlignment = Alignment.Center) {
        Text("$count", style = Type.caption2.bold, color = if (waiting) Palette.onWarning else Palette.onAccent)
    }
}

/** What goes on, the most pressing first ([botLine]), otherwise the last message (Grok Bot row behavior). */
@Composable private fun Preview(bot: Bot, state: AppState) {
    val line = botLine(bot, translator())
    when {
        line != null -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            val color = if (line.kind == BotLineKind.NeedsYou) Palette.warning else Palette.secondary
            when (line.kind) {
                BotLineKind.NeedsYou -> ThinkingOrb(ThinkingOrbState.Listening, 16.dp, color)
                BotLineKind.Working -> ThinkingOrb(size = 13.dp, color = color)
                BotLineKind.Waiting -> Unit
            }
            Text(line.text, style = Type.subheadline, color = color, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        bot.status == "error" -> Text(messagePreview(bot.lastMessage ?: "Something went wrong"), style = Type.subheadline, color = Palette.danger,
            maxLines = 1, overflow = TextOverflow.Ellipsis)
        else -> Text(messagePreview(bot.lastMessage ?: if (bot.kind == "group") "${bot.members.size} bots"
            else "${backendName(state, bot.backend)} · ${bot.folderName ?: tr("Own folder")}"),
            style = Type.subheadline, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** The first non-empty line, without Markdown markers (CodyncKit `MessagePreview`). */
internal fun messagePreview(text: String): String {
    val first = text.lines().map(String::trim).firstOrNull(String::isNotEmpty).orEmpty()
    return first.dropWhile { it in "#>-* " }.replace("**", "").replace("`", "")
}

@Composable private fun EmptyRoster(canCreate: Boolean, hasComputer: Boolean, create: () -> Unit, showComputers: () -> Unit) {
    Column(Modifier.fillMaxWidth().padding(vertical = 40.dp), horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(14.dp)) {
        CharacterAvatar("cloud", "green", 72.dp)
        Text("No bots yet", style = Type.title3.semibold, color = Palette.text)
        Text(if (hasComputer) "Create a bot for each kind of work — a reviewer, a fixer, a docs writer — and point it at a project."
            else "Ask one of your account's computers for access, or pair one with its code.",
            style = Type.subheadline, color = Palette.secondary, textAlign = TextAlign.Center)
        if (canCreate) PrimaryButton("Create your first bot", action = create)
        else if (!hasComputer) PrimaryButton("Computers", action = showComputers)
    }
}

/** The account capsule: who's signed in, opening the account sheet. */
@Composable internal fun AccountSwitcherButton(state: AccountContext, open: () -> Unit) {
    val (source, pressed) = rememberPress()
    val email = state.account.current?.email
    Row(Modifier.height(44.dp).glass(Capsule(), pressed = pressed)
        .clickable(source, null, role = Role.Button, onClick = open)
        .semantics { contentDescription = "Switch account"; stateDescription = email ?: "Local pairing" }
        .padding(start = 7.dp, end = 12.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        AccountAvatar(state, 30.dp)
        Symbol("chevron.down", size = 21.dp, tint = Palette.secondary)
    }
}

@Composable private fun NewMenuButton(enabled: Boolean, groupEnabled: Boolean, newBot: () -> Unit, newGroup: () -> Unit) {
    DropdownMenu({
        listOf(MenuItem("New bot", icon = "plus", action = newBot),
            MenuItem("New group chat", icon = "person.2", enabled = groupEnabled, action = newGroup))
    }, Modifier.size(44.dp).glass(Capsule()), enabled = enabled, label = "New") {
        Symbol("plus", size = 30.dp, tint = Palette.text.copy(alpha = if (enabled) 1f else 0.35f))
    }
}

internal fun backendName(state: AppState, id: String): String =
    state.hello?.get("backends")?.let { it as? kotlinx.serialization.json.JsonArray }
        ?.firstOrNull { (it as? kotlinx.serialization.json.JsonObject)?.get("id") == JsonPrimitive(id) }
        ?.let { ((it as kotlinx.serialization.json.JsonObject)["name"] as? JsonPrimitive)?.content }
        ?: when (id) { "claude" -> "Claude Code"; "codex" -> "Codex"; "opencode" -> "OpenCode"; "grok" -> "Grok Build"; "gemini" -> "Gemini CLI"; else -> "Custom agent" }
