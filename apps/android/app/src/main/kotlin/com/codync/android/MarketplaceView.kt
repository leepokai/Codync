package com.codync.android

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
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
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.core.AgentBackend
import com.codync.android.core.MarketConnector
import com.codync.android.core.MarketSkill
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/**
 * Extensions (iOS `MarketplaceView`): ready-made bots and coordinator teams, then Agents, Connectors and
 * Skills one at a time, each a lazy list; the connectors load more from the registry as their end comes
 * into view. Installing happens on the computer; each bot turns connectors and skills on in its settings.
 */
@Composable internal fun MarketplaceView(
    state: AppState,
    store: AppStore,
    inModal: Boolean = true,
    bottomInset: Dp = 0.dp,
    active: Boolean = true,
    reselectToken: Int = 0,
    openComputers: (() -> Unit)? = null,
) {
    var showInstalled by remember { mutableStateOf(false) }
    LaunchedEffect(reselectToken) {
        if (reselectToken > 0 && showInstalled) showInstalled = false
    }
    AnimatedContent(showInstalled, transitionSpec = {
        (slideInHorizontally(Motion.layoutOffset) { if (targetState) it else -it } + fadeIn(Motion.fade())) togetherWith
            (slideOutHorizontally(Motion.layoutOffset) { if (targetState) -it else it } + fadeOut(Motion.fade()))
    }, label = "Extensions") { installed ->
        if (installed) InstalledView(state, store, inModal = inModal, bottomInset = bottomInset) { showInstalled = false }
        else MarketplacePage(state, store, inModal, bottomInset, active, reselectToken, openComputers) { showInstalled = true }
    }
}

@Composable private fun MarketplacePage(
    state: AppState,
    store: AppStore,
    inModal: Boolean,
    bottomInset: Dp,
    active: Boolean,
    reselectToken: Int,
    openComputers: (() -> Unit)?,
    showInstalled: () -> Unit,
) {
    val dismiss = LocalDismissModal.current
    val scope = rememberCoroutineScope()
    var everActive by rememberSaveable { mutableStateOf(inModal) }
    if (active || inModal) everActive = true
    var search by remember { mutableStateOf("") }
    // Bumped on each submitted search, so every section searches again.
    var searchToken by remember { mutableIntStateOf(0) }
    val connectors = remember(store) {
        ConnectorCatalog<MarketConnector>({ it.name }) { search, cursor -> store.marketConnectors(search, cursor).let { it.items to it.nextCursor } }
    }
    var skills by remember { mutableStateOf(emptyList<MarketSkill>()) }
    var loadingSkills by remember { mutableStateOf(true) }
    var error by remember { mutableStateOf<String?>(null) }
    var installing by remember { mutableStateOf<MarketConnector?>(null) }
    var viewingConnector by remember { mutableStateOf<MarketConnector?>(null) }
    var busySkill by remember { mutableStateOf<String?>(null) }
    var addingConnector by remember { mutableStateOf(false) }
    var writingSkill by remember { mutableStateOf(false) }
    var viewingSkill by remember { mutableStateOf<SkillRef?>(null) }
    var agent by remember { mutableStateOf<AgentBackend?>(null) }
    var addingNode by remember { mutableStateOf(false) }
    var showCredentials by remember { mutableStateOf(false) }
    var section by remember { mutableStateOf(ExtensionSection.Agents) }
    val query = search.trim()
    // Nothing loads until Extensions is on screen on a phone with a computer; a sheet is already on it.
    val canLoad = everActive && (inModal || state.computers.isNotEmpty())
    fun submit() { searchToken++; if (canLoad) scope.launch { connectors.load(query) } }
    LaunchedEffect(canLoad, state.contextId) {
        if (!canLoad) return@LaunchedEffect
        store.refreshPlugins()
        store.refreshBackends()
    }
    LaunchedEffect(canLoad, state.contextId) {
        if (!canLoad) return@LaunchedEffect
        connectors.load(query)
    }
    LaunchedEffect(canLoad, state.contextId) {
        if (!canLoad) return@LaunchedEffect
        loadingSkills = true
        try { skills = store.marketSkills() }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "Couldn't load skills." }
        finally { loadingSkills = false }
    }
    val backends = agentBackends(state)
    val agents = backends.filter { it.available || it.installed || it.curated }.filter { query.isEmpty() || it.name.contains(query, ignoreCase = true) }
    val installedSkills = state.plugins?.skills.orEmpty()
    val shownSkills = if (query.isEmpty()) skills else skills.filter { it.name.contains(query, true) || it.description.contains(query, true) }
    val side = if (inModal) 44.dp else 24.dp
    val inset = Modifier.padding(horizontal = side)
    val needsNode = backends.any { it.needsNode }
    val pinned = section != ExtensionSection.Agents || needsNode
    // Each section opens at its top.
    val listState = remember(section) { LazyListState() }
    // A second tap on Extensions brings the open section back to its top.
    LaunchedEffect(reselectToken) {
        if (reselectToken > 0) listState.animateScrollToItem(0)
    }
    /** Ready-made bots and coordinator teams: they create bots, so they sit above every section. */
    fun LazyListScope.readyMade() {
        if (canLoad) item { Box(Modifier.padding(bottom = 12.dp)) { CatalogSections(state, store, query, Modifier) } }
    }
    /** Apps through Composio under Connectors: connecting one adds it like any other connector. */
    fun LazyListScope.apps() {
        if (section == ExtensionSection.Connectors && canLoad) item {
            Box(Modifier.padding(top = 12.dp)) { ComposioSection(query, searchToken, state, store) }
        }
    }
    @Composable fun Pins() {
        if (section == ExtensionSection.Agents && needsNode) NodeNotice(state) { addingNode = true }
        if (section == ExtensionSection.Connectors) MarketRow("Custom connector", "Any MCP server: a command or a URL", added = false, addLabel = "New",
            icon = { TileIcon("point.3.connected.trianglepath.dotted") }) { addingConnector = true }
        if (section == ExtensionSection.Skills) MarketRow("Your own skill", "Write instructions a bot can follow", added = false, addLabel = "New",
            icon = { TileIcon("square.and.pencil") }) { writingSkill = true }
    }
    /** The open section's list as lazy items, two agent tiles to a row, so a long list composes only the rows near view. */
    fun LazyListScope.lists() {
        when (section) {
            ExtensionSection.Agents -> if (agents.isEmpty()) item {
                Text(if (query.isEmpty()) tr("No agents yet.") else tr("No agents match “{query}”.").replace("{query}", query), style = Type.body, color = Palette.secondary)
            } else items(agents.chunked(2)) { row ->
                Row(Modifier.padding(bottom = 6.dp), horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    for (backend in row) AgentCard(backend, Modifier.weight(1f)) { agent = backend }
                    if (row.size == 1) Spacer(Modifier.weight(1f))
                }
            }
            ExtensionSection.Connectors -> when {
                connectors.loading -> item { SkeletonRows() }
                connectors.items.items.isEmpty() && !connectors.hasMore -> item {
                    Text(if (query.isEmpty()) tr("Couldn't load connectors.") else tr("No connectors match “{query}”.").replace("{query}", query), style = Type.body, color = Palette.secondary)
                }
                else -> {
                    val rows = connectors.items.items
                    items(rows, key = { it.name }) { c ->
                        MarketRow(c.title, c.description ?: c.name, c.installed, open = if (c.installed) null else ({ viewingConnector = c }),
                            icon = { ServiceLogo(c.website, c.title, c.name) }) { installing = c }
                    }
                    connectors.cursor?.let { cursor -> item { LoadMoreRow(cursor, connectors.error != null) { connectors.loadMore() } } }
                }
            }
            ExtensionSection.Skills -> if (loadingSkills) item { SkeletonRows() }
            else if (query.isNotEmpty() && shownSkills.isEmpty()) item {
                Text(tr("No skills match “{query}”.").replace("{query}", query), style = Type.body, color = Palette.secondary)
            } else items(shownSkills, key = { it.source }) { s ->
                val added = s.installed || installedSkills.any { it.id == s.source.lowercase() }
                MarketRow(s.name, s.description, added, busy = busySkill == s.source,
                    open = { viewingSkill = SkillRef("catalog/${s.source}", s.name, s.description, s.source, added) }, icon = { SkillGlyph() }) {
                    busySkill = s.source
                    scope.launch {
                        try { store.installSkill(s.source) }
                        catch (cancelled: CancellationException) { throw cancelled }
                        catch (failure: Exception) { error = failure.message ?: "Couldn't add ${s.name}." }
                        finally { busySkill = null }
                    }
                }
            }
        }
    }
    @Composable fun SourceLine() {
        Text(tr(section.source).replace("{name}", state.hostName), style = Type.footnote, color = Palette.tertiary)
    }
    @Composable fun Foot() {
        (error ?: connectors.error)?.let { Text(tr(it), style = Type.footnote, color = Palette.danger) }
    }
    Box(Modifier.fillMaxSize().background(Palette.background).then(if (inModal) Modifier else Modifier.statusBarsPadding())) {
        BoxWithConstraints(Modifier.fillMaxSize()) {
            val wide = maxWidth >= 760.dp
            Column(Modifier.fillMaxSize().padding(top = 20.dp, bottom = 20.dp + bottomInset), verticalArrangement = Arrangement.spacedBy(20.dp)) {
                Header(state, inset, showInstalled) { showCredentials = true }
                if (!inModal && state.computers.isEmpty()) {
                    Column(Modifier.weight(1f).then(inset), horizontalAlignment = Alignment.CenterHorizontally,
                        verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        MarketEmpty("desktopcomputer", "No computer paired", "Pair a computer to browse and install agents, connectors and skills.")
                        if (openComputers != null) SecondaryButton("Open computers", action = openComputers)
                    }
                } else {
                    SearchBox(search, { search = it }, inset, ::submit) { search = ""; submit() }
                    if (wide) Row(Modifier.weight(1f).padding(horizontal = side), horizontalArrangement = Arrangement.spacedBy(20.dp)) {
                        ExtensionRail(section) { section = it }
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            SourceLine()
                            Pins()
                            LazyColumn(Modifier.weight(1f), listState, verticalArrangement = Arrangement.spacedBy(4.dp)) {
                                readyMade()
                                lists()
                                apps()
                            }
                            Foot()
                        }
                    } else LazyColumn(Modifier.weight(1f), listState, contentPadding = PaddingValues(horizontal = side),
                        verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        item {
                            Column(Modifier.padding(bottom = 12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                                TabChoice(section, ExtensionSection.entries.map { it to it.title }, { section = it })
                                SourceLine()
                            }
                        }
                        if (pinned) item { Box(Modifier.padding(bottom = 12.dp)) { Pins() } }
                        readyMade()
                        lists()
                        apps()
                        item { Foot() }
                    }
                }
            }
        }
        // The sheet's own close, over the page when presented as a modal.
        if (inModal) {
            Box(Modifier.align(Alignment.TopEnd).padding(8.dp)) { IconButton("Close", "xmark", action = dismiss) }
        }
    }
    CodyncSheet(installing, { installing = null }) { InstallConnectorSheet(it, state, store) { submit() } }
    CodyncSheet(viewingConnector, { viewingConnector = null }) { InstallConnectorSheet(it, state, store, addsAtOnce = false) { submit() } }
    CodyncSheet(showCredentials, { showCredentials = false }) { CredentialsView(state, store) }
    CodyncSheet(addingConnector, { addingConnector = false }) { CustomConnectorSheet(state, store) }
    CodyncSheet(viewingSkill, { viewingSkill = null }) { SkillSheet(it, state, store) }
    CodyncSheet(writingSkill, { writingSkill = false }) { NewSkillSheet(store) }
    CodyncSheet(agent, { agent = null }) { AgentSheet(it, state, store) }
    CodyncSheet(addingNode, { addingNode = false }) { SetupTerminal(null, "node", null, state, store) { addingNode = false } }
}

/** The computer it all lives on, credentials beside the title, and the way to what's installed. */
@Composable private fun Header(state: AppState, modifier: Modifier, showInstalled: () -> Unit, showCredentials: () -> Unit) {
    val connectors = state.plugins?.connectors.orEmpty()
    val count = connectors.size + state.plugins?.skills.orEmpty().size
    Column(modifier.padding(top = 28.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            ComputerBadge(state.hello?.text("device"), 32.dp, state.computers.firstOrNull()?.color)
            Column(Modifier.weight(1f)) {
                Text(tr("Extensions"), style = Type.title2.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(state.hostName, style = Type.subheadline, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            // Beside the title, in the room left of Close, so the list can fill the screen.
            SecondaryButton(showCredentials) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Symbol("lock.shield", size = 18.dp, tint = Palette.text)
                    Text(tr("Credentials"), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
        if (count > 0) Row(Modifier.plainButton(action = showInstalled), verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(horizontalArrangement = Arrangement.spacedBy((-8).dp)) {
                for (c in connectors.take(3)) ServiceLogo(null, c.name, c.registryName, 26.dp)
            }
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text("$count installed", style = Type.body, color = Palette.secondary)
                Text("Connectors & skills", style = Type.caption, color = Palette.secondary)
            }
            Symbol("chevron.right", size = 13.dp, tint = Palette.tertiary)
        }
    }
}

@Composable private fun SearchBox(text: String, onChange: (String) -> Unit, modifier: Modifier, submit: () -> Unit, clear: () -> Unit) {
    Row(modifier.fillMaxWidth().height(38.dp).clip(Capsule()).background(Palette.bubbleAgent).padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Symbol("magnifyingglass", size = 18.dp, tint = Palette.secondary)
        PlainTextField(text, onChange, Modifier.weight(1f), "Search agents, connectors and skills", singleLine = true,
            imeAction = ImeAction.Search, actions = KeyboardActions(onSearch = { submit() }))
        if (text.isNotEmpty()) Box(Modifier.plainButton(action = clear).semantics { contentDescription = "Clear search" }) {
            Symbol("xmark.circle.fill", size = 18.dp, tint = Palette.tertiary)
        }
    }
}

/** Above the agents while some of them run on Node.js and the computer has none: one place to add Codync's copy for all. */
@Composable private fun NodeNotice(state: AppState, add: () -> Unit) {
    Row(Modifier.fillMaxWidth().clip(RoundedRectangle(18.dp)).background(Palette.bubbleAgent.copy(alpha = 0.45f))
        .padding(horizontal = 14.dp, vertical = 12.dp), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Symbol("shippingbox", Modifier.padding(top = 1.dp), size = 18.dp, tint = Palette.secondary)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text("Node.js isn't on ${state.hostName}", style = Type.subheadline.semibold, color = Palette.text)
                Text("Some agents need it to run. Codync can add a copy just for them.", style = Type.caption, color = Palette.secondary)
            }
            Text("Add Node.js", Modifier.clip(Capsule()).background(Palette.accentFill).plainButton(action = add).padding(horizontal = 14.dp, vertical = 7.dp),
                style = Type.subheadline.semibold, color = Palette.onAccent)
        }
    }
}
