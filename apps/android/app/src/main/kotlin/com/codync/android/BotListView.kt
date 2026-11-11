package com.codync.android

import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.snap
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import coil3.compose.AsyncImage
import com.codync.android.core.Bot
import com.codync.android.core.LinkState
import com.codync.android.core.mixesAgents
import com.codync.android.core.runningAgent
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import kotlin.math.abs
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.putJsonArray

/** The roster, as iOS `BotListView` draws it: every bot is a person you can message. */
@Composable internal fun BotListView(
    state: AppState,
    store: AppStore,
    account: () -> Unit,
    manage: () -> Unit,
    newBot: () -> Unit,
    newGroup: () -> Unit,
    edit: (Bot) -> Unit,
    bottomInset: Dp,
    reselectToken: Int = 0,
    highlightSelection: Boolean = false,
    onDragActive: (Boolean) -> Unit = {},
) {
    val context = LocalContext.current
    val preferences = remember(context) { RosterPreferences(context.applicationContext) }
    val reduced = reduceMotion()
    var confirmDelete by remember { mutableStateOf<Bot?>(null) }
    var searchQuery by rememberSaveable(state.contextId) { mutableStateOf("") }
    var activeFilter by rememberSaveable(state.contextId) { mutableStateOf(RosterFilter.All) }
    var searchReqSeq by remember { mutableLongStateOf(0L) }
    val allRoster = roster(state.bots)
    val online = state.actualConnection is LinkState.Ready
    var searchState by remember(state.contextId) { mutableStateOf(RosterSearchState(botMatches = allRoster)) }

    LaunchedEffect(searchQuery, allRoster, state.entries, state.historyComplete, online) {
        val nextId = searchReqSeq + 1L
        searchReqSeq = nextId
        val computed = if (searchQuery.isNotBlank()) {
            delay(140L)
            withContext(Dispatchers.Default) {
                searchCachedMessages(
                    bots = allRoster,
                    entries = state.entries,
                    historyComplete = state.historyComplete,
                    online = online,
                    query = searchQuery,
                    requestId = nextId,
                    state = state,
                )
            }
        } else {
            RosterSearchState(query = "", requestId = nextId, botMatches = allRoster)
        }
        if (computed.requestId == searchReqSeq) {
            searchState = computed
        }
    }

    val filteredRoster = remember(allRoster, activeFilter, searchQuery, state.hello) {
        filterRosterBots(allRoster, activeFilter, searchQuery, state)
    }
    val attentionItems = remember(allRoster, state.entries, state.busy) {
        resolveAttentionItems(allRoster, state.entries, state.busy)
    }
    val anyWorking = remember(filteredRoster) { filteredRoster.any { it.status == "working" && it.startedAt != null } }
    val nowMillis = rememberRosterClock(active = anyWorking)

    val reorder = remember { RosterReorder() }
    reorder.roster = allRoster.map(Bot::id)
    reorder.lead = allRoster.firstOrNull { it.lead }?.id
    LaunchedEffect(reorder.dragged != null) { onDragActive(reorder.dragged != null) }
    val listState = rememberLazyListState()
    var handledReselect by remember { mutableIntStateOf(reselectToken) }
    LaunchedEffect(reselectToken) {
        if (reselectToken != handledReselect) {
            handledReselect = reselectToken
            if (listState.layoutInfo.totalItemsCount > 0) listState.animateScrollToItem(0)
        }
    }
    val canReorder = activeFilter == RosterFilter.All && searchQuery.isBlank() && !preferences.groupByProject
    val clearFilters = {
        searchQuery = ""
        activeFilter = RosterFilter.All
    }

    val toggleSection = { name: String -> folded = if (name in folded) folded - name else folded + name; FoldedSections.save(context, folded) }
    val agentLogos by AppearancePreferences.agentLogos.collectAsStateWithLifecycle()
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    val contentBottom = bottomInset + 64.dp
    GlassStage(Modifier.fillMaxSize().background(Palette.background), chrome = {
        ScrollEdge(Edge.Top, top + 64.dp)
        Row(
            Modifier.fillMaxWidth().statusBarsPadding().padding(top = 6.dp).padding(horizontal = 16.dp).height(44.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            AccountSwitcherButton(state, account)
            Box(Modifier.weight(1f), contentAlignment = Alignment.Center) { ComputerFilterHeader(state, store, manage) }
            Spacer(Modifier.size(48.dp))
        }
        NewMenuButton(
            enabled = state.computers.isNotEmpty(),
            groupEnabled = online,
            newBot = newBot,
            newGroup = newGroup,
            modifier = Modifier.align(Alignment.BottomEnd).padding(end = 16.dp, bottom = bottomInset + 8.dp),
        )
    }) {
        // Pull to refresh reconnects and asks the account for its computers again (iOS `refreshable`).
        PullToRefresh({ store.reconnect(); store.refreshComputers() }, Modifier.fillMaxSize(), indicatorTop = top + 50.dp) {
            Column(Modifier.fillMaxSize().padding(top = top + 56.dp)) {
                RosterPresentationHeader(
                    allRoster,
                    preferences.presentation,
                    preferences::select,
                    Modifier.padding(horizontal = 16.dp).padding(bottom = 8.dp),
                )
                if (allRoster.isNotEmpty() || searchQuery.isNotBlank() || activeFilter != RosterFilter.All) {
                    RosterControlsHeader(
                        query = searchQuery,
                        onQueryChange = { searchQuery = it },
                        filter = activeFilter,
                        onFilterSelect = { activeFilter = it },
                        groupByProject = preferences.groupByProject,
                        onGroupByProjectChange = preferences::updateGroupByProject,
                        showGroupByProject = preferences.presentation == RosterPresentation.List,
                        bots = allRoster,
                        Modifier.padding(horizontal = 16.dp).padding(bottom = 8.dp),
                    )
                }
                RosterQuickActions(
                    query = searchQuery, online = online, account = account, manage = manage,
                    newBot = newBot, newGroup = newGroup,
                    modifier = Modifier.padding(horizontal = 16.dp).padding(bottom = 8.dp),
                )
                state.mismatch?.let { UpdateNeededCard(state, it, store::requestHostUpdate, Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) }
                Crossfade(
                    preferences.presentation,
                    Modifier.weight(1f),
                    animationSpec = if (reduced) snap() else Motion.fade(),
                    label = "Roster presentation",
                ) { presentation ->
                    if (presentation == RosterPresentation.Constellation && allRoster.isNotEmpty()) {
                        if (filteredRoster.isEmpty()) {
                            Box(Modifier.fillMaxSize().padding(horizontal = 16.dp)) {
                                EmptyFilteredRoster(
                                    query = searchQuery,
                                    filter = activeFilter,
                                    isPartialCache = searchState.isPartialCache,
                                    error = searchState.error,
                                    onClear = clearFilters,
                                )
                            }
                        } else {
                            BotConstellation(
                                bots = filteredRoster,
                                open = { store.openBot(it.id) },
                                actions = { bot -> botRosterActions(bot, store, edit = { edit(bot) }, delete = { confirmDelete = bot }) },
                                modifier = Modifier.fillMaxSize().padding(bottom = contentBottom),
                                totalCount = allRoster.size,
                                onClearFilters = if (filteredRoster.size < allRoster.size) clearFilters else null,
                                reselectToken = reselectToken,
                            )
                        }
                    } else {
                        RosterListContent(
                            state = state,
                            store = store,
                            preferences = preferences,
                            allRoster = allRoster,
                            filteredRoster = filteredRoster,
                            attentionItems = attentionItems,
                            searchQuery = searchQuery,
                            activeFilter = activeFilter,
                            searchState = searchState,
                            reorder = reorder,
                            canReorder = canReorder,
                            nowMillis = nowMillis,
                            bottomInset = contentBottom,
                            listState = listState,
                            highlightSelection = highlightSelection,
                            newBot = newBot,
                            manage = manage,
                            edit = edit,
                            onDelete = { confirmDelete = it },
                            onClearFilters = clearFilters,
                        )
                    }
                }
                if (grouped && (movingComputer || id in collapsed)) continue
                botLines(state, store, roster, folded, grouped, agentLogos, RosterActions(toggleSection,
                    rename = { name -> naming = SectionNaming("Rename section", name) { renameSection(roster, store, name, it) } },
                    remove = { name -> renameSection(roster, store, name, null) },
                    newSection = { bot -> naming = SectionNaming("New section", "") { store.setSection(bot.id, it) } },
                    edit = { bot -> edit(id, bot) }, delete = { bot -> confirmDelete = id to bot }))
            }
        }
    }
    val deleting = confirmDelete
    CodyncDialog(
        "Delete ${deleting?.name ?: "bot"}?",
        deleting != null,
        { confirmDelete = null },
        message = if (deleting?.kind == "group") "Its bots and their own chats stay." else "Files it changed on your computer stay as they are.",
    ) {
        listOf(
            DialogAction(if (deleting?.kind == "group") "Delete group chat" else "Delete bot and its conversation", destructive = true) {
                deleting?.let { store.deleteBot(it) {} }
            },
        )
    }
}

@Composable private fun RosterListContent(
    state: AppState,
    store: AppStore,
    preferences: RosterPreferences,
    allRoster: List<Bot>,
    filteredRoster: List<Bot>,
    attentionItems: List<BotAttentionItem>,
    searchQuery: String,
    activeFilter: RosterFilter,
    searchState: RosterSearchState,
    reorder: RosterReorder,
    canReorder: Boolean,
    nowMillis: Long,
    bottomInset: Dp,
    listState: LazyListState,
    highlightSelection: Boolean,
    newBot: () -> Unit,
    manage: () -> Unit,
    edit: (Bot) -> Unit,
    onDelete: (Bot) -> Unit,
    onClearFilters: () -> Unit,
) {
    val haptics = LocalCodyncHaptics.current
    val computerId = state.computer?.id ?: "unpaired"
    val computerName = state.hello?.text("name") ?: state.computer?.name

    LazyColumn(
        Modifier.fillMaxSize(),
        state = listState,
        contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 0.dp, bottom = bottomInset),
    ) {
        if (attentionItems.isNotEmpty() && searchQuery.isBlank()) {
            item(key = "attention-banner") {
                AttentionBannerSection(
                    items = attentionItems,
                    bots = state.bots,
                    store = store,
                    modifier = Modifier.padding(bottom = 8.dp),
                )
            }
        }
        if (allRoster.isEmpty() && state.mismatch == null) {
            item(key = "empty-roster") {
                EmptyRoster(
                    canCreate = state.computers.isNotEmpty(),
                    hasComputer = state.computers.isNotEmpty(),
                    create = newBot,
                    showComputers = manage,
                )
            }
        } else if (filteredRoster.isEmpty() && searchState.messageHits.isEmpty()) {
            item(key = "empty-filtered") {
                EmptyFilteredRoster(
                    query = searchQuery,
                    filter = activeFilter,
                    isPartialCache = searchState.isPartialCache,
                    error = searchState.error,
                    onClear = onClearFilters,
                )
            }
        } else if (preferences.groupByProject) {
            val sections = groupBotsByWorkspace(filteredRoster, computerId, computerName)
            for (section in sections) {
                val collapsed = preferences.isSectionCollapsed(state.contextId, section.key)
                item(key = "section:${section.key.storageKey}") {
                    WorkspaceSectionHeader(
                        section = section,
                        collapsed = collapsed,
                        onToggle = {
                            haptics.perform(CodyncHaptic.Toggle)
                            preferences.toggleSectionCollapsed(state.contextId, section.key)
                        },
                    )
                }
                if (!collapsed) {
                    items(section.bots, key = { "grouped:${section.key.storageKey}:${it.id}" }) { bot ->
                        RosterBotListEntry(
                            bot = bot,
                            state = state,
                            store = store,
                            reorderActive = false,
                            drag = null,
                            nowMillis = nowMillis,
                            edit = { edit(bot) },
                            delete = { onDelete(bot) },
                            selected = highlightSelection && state.selectedBot == bot.id,
                        )
                    }
                }
            }
        } else {
            val shownBots = if (canReorder) reorder.shown(filteredRoster) else filteredRoster
            items(shownBots, key = Bot::id) { bot ->
                val lifted = reorder.dragged == bot.id
                Box(
                    Modifier.onSizeChanged { reorder.heights[bot.id] = it.height }
                        .then(
                            if (lifted) {
                                Modifier.zIndex(1f).graphicsLayer {
                                    translationY = reorder.offset
                                    scaleX = 1.02f
                                    scaleY = 1.02f
                                }
                            } else {
                                Modifier.animateItem()
                            },
                        ),
                ) {
                    RosterBotListEntry(
                        bot = bot,
                        state = state,
                        store = store,
                        reorderActive = reorder.dragged != null,
                        drag = if (canReorder && !bot.lead) reorder.handle(bot.id, haptics) { store.reorder(it) } else null,
                        nowMillis = nowMillis,
                        edit = { edit(bot) },
                        delete = { onDelete(bot) },
                        selected = highlightSelection && state.selectedBot == bot.id,
                    )
    }
    val deleting = confirmDelete?.second
    CodyncDialog("Delete ${deleting?.name ?: "bot"}?", deleting != null, { confirmDelete = null },
        message = if (deleting?.kind == "group") "Its bots and their own chats stay." else "Files it changed on your computer stay as they are.") {
        listOf(DialogAction(if (deleting?.kind == "group") "Delete group chat" else "Delete bot and its conversation", destructive = true) {
            confirmDelete?.let { (id, bot) -> accounts.store(id)?.deleteBot(bot) {} }
        })
    }
    SectionNameDialog(naming) { naming = null }
}

/** A computer's heading (and, while it needs one, its update notice) that drags to reorder the computers. */
@Composable private fun LazyItemScope.ComputerHeading(state: AppState, store: AppStore, reorder: SectionReorder, folded: Boolean, stale: Boolean,
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
    val newSection: (Bot) -> Unit, val edit: (Bot) -> Unit, val delete: (Bot) -> Unit)

/** One computer's bots: pinned ones, then each section's heading with its bots (none while folded), then the bots in no section. */
private fun LazyListScope.botLines(state: AppState, store: AppStore, roster: ComputerRoster, folded: Set<String>, grouped: Boolean,
    agentLogos: Boolean, actions: RosterActions) {
    val id = state.computer.id
    val sectionReorder = roster.sections
    val reorder = roster.reorder
    val sections = menuSections(state.bots)
    // The agent's logo marks each avatar only while the computer's bots mix agents. Not before hello: it names
    // the logos, and without it every badge would be the same generic one.
    val logos = agentLogos && state.hello != null && roster(state.bots).mixesAgents()
    items(rosterLines(reorder.shown(roster(state.bots)), folded, sectionReorder.shown, sectionReorder.dragged != null), key = { "$id:${it.key}" }) { line ->
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
                    BotRowItem(bot, state, store, reorder.handle(bot.id) { store.reorder(it) }, sections, grouped, agent, edit = { actions.edit(bot) },
                        delete = { actions.delete(bot) }, newSection = { actions.newSection(bot) })
                }
            }
        }
        if (searchQuery.isNotBlank() && searchState.messageHits.isNotEmpty()) {
            item(key = "message-hits") {
                MessageSearchHitsSection(
                    searchState = searchState,
                    bots = state.bots,
                    onOpenHit = { hit -> store.openBot(hit.botId, hit.threadId?.takeIf { it.startsWith("doc:") }) },
                )
            }
        }
    }
}

/**
 * Drag-to-reorder for the roster (iOS `List.onMove`): long-press a row, then move it. The order
 * shown follows the finger until the host's synced order catches up.
 */
internal class RosterReorder {
    var roster: List<String> = emptyList()
    /** Stays on top: nothing takes its place. */
    var lead: String? = null
    var order by mutableStateOf<List<String>?>(null)
    var dragged by mutableStateOf<String?>(null)
    var offset by mutableFloatStateOf(0f)
    val heights = mutableMapOf<String, Int>()
    private val handles = mutableMapOf<String, ContextDrag>()

    fun shown(bots: List<Bot>): List<Bot> {
        val ids = order ?: return bots
        // Dropped and synced: the host's order is the one to show again.
        if (dragged == null && ids == roster) { order = null; return bots }
        return ids.mapNotNull { id -> bots.firstOrNull { it.id == id } } + bots.filter { it.id !in ids }
    }

    fun handle(id: String, haptics: CodyncHaptics, save: (List<String>) -> Unit): ContextDrag = handles.getOrPut(id) {
        ContextDrag(start = { order = order ?: roster; dragged = id; offset = 0f }, move = { step -> offset += step; swap(id, haptics) }, end = {
            val dropped = order
            dragged = null; offset = 0f
            if (dropped != null && dropped != roster) save(dropped) else order = null
        })
    }

    /** Past half the neighbor's height, the row takes the neighbor's place. */
    internal fun swap(id: String, haptics: CodyncHaptics? = null) {
        val list = order ?: return
        val at = list.indexOf(id)
        val next = if (offset > 0) at + 1 else at - 1
        if (list.getOrNull(next) == lead) return
        val height = list.getOrNull(next)?.let(heights::get) ?: return
        if (abs(offset) < height / 2f) return
        order = list.toMutableList().apply { add(next, removeAt(at)) }
        offset -= if (offset > 0) height else -height
        haptics?.perform(CodyncHaptic.DragStep)
    }
}

/** Roster order: the lead, then pinned bots, then where the user dragged them, then most recent activity. */
internal fun roster(bots: List<Bot>): List<Bot> = bots.filter { !it.deleted && !it.hidden }
    .sortedWith(compareBy<Bot> { it.band }.thenBy { it.position }.thenByDescending { it.lastAt })

/** The roster order with a bot one place up (-1) or down (+1) within its band (`Bot.band`), or null when it can't move. */
internal fun moved(roster: List<Bot>, id: String, offset: Int): List<String>? {
    val at = roster.indexOfFirst { it.id == id }
    val neighbor = roster.getOrNull(at + offset)
    if (at < 0 || neighbor == null || neighbor.band != roster[at].band) return null
    return roster.map(Bot::id).toMutableList().apply { this[at] = neighbor.id; this[at + offset] = id }
    if (at < 0) return null
    val next = generateSequence(at + offset) { it + offset }.takeWhile { it in roster.indices }
        .firstOrNull { roster[it].movesWith(roster[at]) } ?: return null
    return roster.map(Bot::id).toMutableList().apply { this[at] = roster[next].id; this[next] = id }
}

private fun Bot.movesWith(other: Bot) = band == other.band && (pinned || section == other.section)

@Composable private fun BotRowItem(bot: Bot, state: AppState, store: AppStore, drag: ContextDrag, sections: List<String>, grouped: Boolean,
    agent: AgentMark?, edit: () -> Unit, delete: () -> Unit, newSection: () -> Unit) {
    Box(Modifier.fillMaxWidth().contextActions(onClick = { store.openBot(bot.id) }, reorder = drag, preview = {
        Box(Modifier.padding(horizontal = 8.dp)) { BotRow(bot, state, agent) }
    }) {
        listOfNotNull(
            MenuItem(if (bot.pinned) "Unpin" else "Pin", icon = if (bot.pinned) "pin.slash" else "pin") {
                store.command("updateBot", buildJsonObject { put("id", bot.id); put("pinned", !bot.pinned) }, "roster:" + bot.id)
            },
            if (state.supportsSections) sectionMenu(bot, sections, { store.setSection(bot.id, it) }, newSection) else null,
            if (bot.kind == "group") MenuItem("Edit group", icon = "person.2", action = edit)
            else MenuItem("Edit profile", icon = "pencil", action = edit),
            MenuItem("Mark as read", icon = "checkmark.message") {
                store.command("markRead", buildJsonObject { put("botId", bot.id) }, "roster:" + bot.id)
            },
            MenuItem("Make lead agent", icon = "star") {
                store.command("setLead", buildJsonObject { put("botId", bot.id) }, "roster:" + bot.id)
            }.takeIf { !bot.lead && bot.kind != "group" && state.supportsLeadChoice },
            // The lead stays in the list.
            MenuItem("Hide from list", icon = "eye.slash") {
                store.command("updateBot", buildJsonObject { put("id", bot.id); put("hidden", true) }, "roster:" + bot.id)
            }.takeUnless { bot.lead },
            // An older computer keeps its lead.
            MenuItem("Delete", icon = "trash", destructive = true, action = delete).takeIf { !bot.lead || state.supportsLeadChoice },
        )
    }.semantics {
        // With several computers, which one it's on.
        contentDescription = if (grouped) "${bot.name}, on ${state.hostName}" else bot.name
        // TalkBack's way to reorder (the drag needs a touch and hold, then a move).
        customActions = listOf(-1 to "Move up", 1 to "Move down").map { (offset, title) ->
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
                if (bot.unread > 0) UnreadBadge(bot.unread)
            }
        }
    }
}

/** A count of unread messages in the accent capsule (a row, a folded section). */
@Composable internal fun UnreadBadge(count: Int) {
    Box(Modifier.height(18.dp).widthIn(min = 18.dp).clip(Capsule()).background(Palette.accentFill)
        .padding(horizontal = 6.dp).semantics { contentDescription = "$count unread" }, contentAlignment = Alignment.Center) {
        Text("$count", style = Type.caption2.bold, color = Palette.onAccent)
    }
}

/** Live activity while working, otherwise the last message (Grok Bot row behavior). */
@Composable private fun Preview(bot: Bot, state: AppState) {
    when {
        bot.status == "needsInput" -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            ThinkingOrb(ThinkingOrbState.Listening, 16.dp, Palette.warning)
            Text(tr(bot.activity.ifEmpty { "Needs your approval" }), style = Type.subheadline, color = Palette.warning, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        bot.status == "working" -> Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            ThinkingOrb(size = 13.dp, color = Palette.secondary)
            Text(tr(bot.activity.ifEmpty { "Working…" }), style = Type.subheadline, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        bot.status == "error" -> Text(messagePreview(bot.lastMessage ?: "Something went wrong"), style = Type.subheadline, color = Palette.danger,
            maxLines = 1, overflow = TextOverflow.Ellipsis)
        else -> Text(messagePreview(bot.lastMessage ?: if (bot.kind == "group") "${bot.members.size} bots"
            else "${backendName(state, bot.backend)} · ${if (bot.managedWorkspace) tr("Personal workspace") else bot.cwd.trimEnd('/').substringAfterLast('/')}"),
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
@Composable internal fun AccountSwitcherButton(state: AppState, open: () -> Unit) {
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

/**
 * The quiet connection summary, with reconnect and the computers on demand. With several computers
 * paired, it names the one shown and switches to another (each keeps its own bots and chats).
 */
@Composable private fun ComputerFilterHeader(state: AppState, store: AppStore, manage: () -> Unit) {
    val summary = connectionSummary(state)
    val shown = state.computer?.id
    val active = state.bots.any { it.status == "working" || it.status == "needsInput" }
    DropdownMenu({
        listOfNotNull(
            MenuItem("All computers", icon = "checkmark").takeIf { state.computers.size <= 1 },
            *state.computers.map { computer ->
                if (computer.id == shown) MenuItem("${state.hostName} · ${connectionLabel(state)}", icon = "checkmark")
                else MenuItem(computer.name, icon = "desktopcomputer") { store.switchComputer(computer.id) }
            }.toTypedArray(),
            MenuItem(
                "Stop all", icon = "stop.fill", divider = true,
                enabled = "stopAll" !in state.busy && state.actualConnection is LinkState.Ready,
            ) { store.stopAllBots() }.takeIf { active },
            MenuItem("Reconnect", icon = "arrow.clockwise", divider = !active, enabled = state.computers.isNotEmpty()) { store.reconnect() },
            MenuItem("Manage computers", icon = "desktopcomputer", action = manage),
        )
    }, label = "Computers, $summary. " + if (state.computers.size > 1) "Switch computer" else "Filter conversations") {
        Row(Modifier.padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(summary, style = Type.system(12f).medium, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Symbol("chevron.down", size = 15.dp, tint = Palette.secondary)
        }
    }
}

/** Contextual creation actions (`New bot` and `New group chat`), shared by the floating action button. */
internal fun newBotMenuItems(groupEnabled: Boolean, newBot: () -> Unit, newGroup: () -> Unit): List<MenuItem> =
    listOf(
        MenuItem("New bot", icon = "plus", action = newBot),
        MenuItem("New group chat", icon = "person.2", enabled = groupEnabled, action = newGroup),
    )

@Composable internal fun NewMenuButton(
    enabled: Boolean,
    groupEnabled: Boolean,
    newBot: () -> Unit,
    newGroup: () -> Unit,
    modifier: Modifier = Modifier,
) {
    DropdownMenu(
        items = { newBotMenuItems(groupEnabled, newBot, newGroup) },
        modifier = modifier.size(48.dp).glass(Capsule()),
        enabled = enabled,
        label = "New",
    ) {
        Symbol("plus", size = 28.dp, tint = Palette.text.copy(alpha = if (enabled) 1f else 0.35f))
    }
}

internal fun backendName(state: AppState? = null, id: String): String =
    state?.hello?.get("backends")?.let { it as? kotlinx.serialization.json.JsonArray }
        ?.firstOrNull { (it as? kotlinx.serialization.json.JsonObject)?.get("id") == JsonPrimitive(id) }
        ?.let { ((it as kotlinx.serialization.json.JsonObject)["name"] as? JsonPrimitive)?.content }
        ?: when (id) { "claude" -> "Claude Code"; "codex" -> "Codex"; "opencode" -> "OpenCode"; "grok" -> "Grok Build"; "gemini" -> "Gemini CLI"; "external" -> "Terminal"; else -> "Custom agent" }

/** Saves a dragged roster order (the host syncs it to every device). */
internal fun AppStore.reorder(ids: List<String>) =
    command("reorderBots", buildJsonObject { putJsonArray("ids") { ids.forEach { add(JsonPrimitive(it)) } } }, "reorder")
