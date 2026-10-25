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
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.text.style.TextOverflow
import com.codync.android.core.AgentBackend
import com.codync.android.core.CatalogPaging
import com.codync.android.core.MarketConnector
import com.codync.android.core.MarketSkill
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/**
 * Extensions (iOS `MarketplaceView`): Agents, Connectors and Skills, one at a time, each paged.
 * Installing happens on the computer; each bot turns connectors and skills on in its settings.
 */
@Composable internal fun MarketplaceView(state: AppState, store: AppStore) {
    var showInstalled by remember { mutableStateOf(false) }
    AnimatedContent(showInstalled, transitionSpec = {
        (slideInHorizontally(Motion.layoutOffset) { if (targetState) it else -it } + fadeIn(Motion.fade())) togetherWith
            (slideOutHorizontally(Motion.layoutOffset) { if (targetState) -it else it } + fadeOut(Motion.fade()))
    }, label = "Extensions") { installed ->
        if (installed) InstalledView(state, store) { showInstalled = false } else MarketplacePage(state, store) { showInstalled = true }
    }
}

@Composable private fun MarketplacePage(state: AppState, store: AppStore, showInstalled: () -> Unit) {
    val dismiss = LocalDismissModal.current
    val scope = rememberCoroutineScope()
    var search by remember { mutableStateOf("") }
    val connectors = remember(store) {
        ConnectorCatalog<MarketConnector>({ it.name }) { search, cursor -> store.marketConnectors(search, cursor).let { it.items to it.nextCursor } }
    }
    var skills by remember { mutableStateOf(emptyList<MarketSkill>()) }
    var loadingSkills by remember { mutableStateOf(true) }
    var error by remember { mutableStateOf<String?>(null) }
    var installing by remember { mutableStateOf<MarketConnector?>(null) }
    var busySkill by remember { mutableStateOf<String?>(null) }
    var addingConnector by remember { mutableStateOf(false) }
    var writingSkill by remember { mutableStateOf(false) }
    var viewingSkill by remember { mutableStateOf<SkillRef?>(null) }
    var agent by remember { mutableStateOf<AgentBackend?>(null) }
    var addingNode by remember { mutableStateOf(false) }
    var showCredentials by remember { mutableStateOf(false) }
    var section by remember { mutableStateOf(ExtensionSection.Agents) }
    var agentPage by remember { mutableIntStateOf(0) }
    var skillPage by remember { mutableIntStateOf(0) }
    var pageSize by remember { mutableIntStateOf(CatalogPaging.phonePageSize) }
    var pickedPageSize by remember { mutableStateOf(false) }
    val query = search.trim()
    /**
     * A size picked from the menu, or one that follows the layout, starts every list over in the same change as
     * the size, so no pager is ever built for the new size at the old page.
     */
    fun resize(size: Int) { pageSize = size; agentPage = 0; skillPage = 0; connectors.show(0, size) }
    fun changePageSize(size: Int) { pickedPageSize = true; resize(size) }
    /** Turns the connector list to `page` at once; a page that isn't full then fills from the registry. */
    fun showConnectorPage(page: Int) {
        connectors.show(page, pageSize)
        scope.launch { connectors.fill(pageSize) }
    }
    fun submit() { scope.launch { connectors.load(query) } }
    LaunchedEffect(Unit) { store.refreshPlugins(); store.refreshBackends() }
    LaunchedEffect(Unit) { connectors.load(query) }
    LaunchedEffect(Unit) {
        try { skills = store.marketSkills() }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "Couldn't load skills." }
        finally { loadingSkills = false }
    }
    val backends = agentBackends(state)
    val agents = backends.filter { it.available || it.installed || it.curated }.filter { query.isEmpty() || it.name.contains(query, ignoreCase = true) }
    val installedSkills = state.plugins?.skills.orEmpty()
    val shownSkills = if (query.isEmpty()) skills else skills.filter { it.name.contains(query, true) || it.description.contains(query, true) }
    val inset = Modifier.padding(horizontal = 44.dp)
    fun changeSearch(next: String) { search = next; agentPage = 0; skillPage = 0 }
    fun pagePad(count: Int, shown: Int) = if (count > pageSize) maxOf(0, pageSize - shown) else 0
    @Composable fun Pins() {
        if (section == ExtensionSection.Agents && backends.any { it.needsNode }) NodeNotice(state) { addingNode = true }
        if (section == ExtensionSection.Connectors) MarketRow("Custom connector", "Any MCP server: a command or a URL", added = false, addLabel = "New",
            icon = { TileIcon("point.3.connected.trianglepath.dotted") }) { addingConnector = true }
        if (section == ExtensionSection.Skills) MarketRow("Your own skill", "Write instructions a bot can follow", added = false, addLabel = "New",
            icon = { TileIcon("square.and.pencil") }) { writingSkill = true }
    }
    @Composable fun Lists() {
        when (section) {
            ExtensionSection.Agents -> if (agents.isEmpty()) Text(if (query.isEmpty()) tr("No agents yet.") else tr("No agents match “{query}”.").replace("{query}", query), style = Type.body, color = Palette.secondary)
            else {
                FlankedPager(agentPage, agents.size, pageSize, settle = { agentPage = it }) { index ->
                    val shown = CatalogPaging.slice(agents, index, pageSize)
                    val slots = buildList<AgentBackend?> {
                        addAll(shown)
                        repeat(pagePad(agents.size, shown.size)) { add(null) }
                    }
                    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        for (row in slots.chunked(2)) Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                            for (backend in row) if (backend != null) AgentCard(backend, Modifier.weight(1f)) { agent = backend }
                            else AgentCardSpacer(Modifier.weight(1f))
                            if (row.size == 1) Spacer(Modifier.weight(1f))
                        }
                    }
                }
            }
            ExtensionSection.Connectors -> when {
                connectors.loading -> SkeletonRows()
                connectors.items.items.isEmpty() -> Text(if (query.isEmpty()) tr("Couldn't load connectors.") else tr("No connectors match “{query}”.").replace("{query}", query), style = Type.body, color = Palette.secondary)
                else -> {
                    val rows = connectors.items.items
                    FlankedPager(connectors.page, rows.size, pageSize, connectors.hasMore, connectors.loadingMore, ::showConnectorPage) { index ->
                        val shown = CatalogPaging.slice(rows, index, pageSize)
                        // The page past the loaded rows, while the registry fills it.
                        if (shown.isEmpty()) SkeletonRows()
                        else Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            for (c in shown) MarketRow(c.title, c.description ?: c.name, c.installed,
                                icon = { ServiceLogo(c.website, c.title, c.name) }) { installing = c }
                            repeat(pagePad(rows.size, shown.size)) { MarketRowSpacer() }
                        }
                    }
                }
            }
            ExtensionSection.Skills -> if (loadingSkills) SkeletonRows()
            else if (query.isNotEmpty() && shownSkills.isEmpty()) Text(tr("No skills match “{query}”.").replace("{query}", query), style = Type.body, color = Palette.secondary)
            else {
                FlankedPager(skillPage, shownSkills.size, pageSize, settle = { skillPage = it }) { index ->
                    val shown = CatalogPaging.slice(shownSkills, index, pageSize)
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        for (s in shown) {
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
                        repeat(pagePad(shownSkills.size, shown.size)) { MarketRowSpacer() }
                    }
                }
            }
        }
    }
    @Composable fun Pagers() {
        when (section) {
            ExtensionSection.Agents -> CatalogPager(agentPage, agents.size, pageSize, go = { agentPage = CatalogPaging.clamp(it, agents.size, pageSize) }, onSize = ::changePageSize)
            ExtensionSection.Connectors -> if (!connectors.loading) CatalogPager(connectors.page, connectors.items.items.size, pageSize, connectors.hasMore,
                connectors.loadingMore, ::showConnectorPage, ::changePageSize)
            ExtensionSection.Skills -> if (!loadingSkills) CatalogPager(skillPage, shownSkills.size, pageSize, go = { skillPage = CatalogPaging.clamp(it, shownSkills.size, pageSize) }, onSize = ::changePageSize)
        }
    }
    @Composable fun SourceLine() {
        Text(tr(section.source).replace("{name}", state.hostName), style = Type.footnote, color = Palette.tertiary)
    }
    @Composable fun Foot() {
        (error ?: connectors.error)?.let { Text(tr(it), style = Type.footnote, color = Palette.danger) }
    }
    Box(Modifier.fillMaxSize().background(Palette.background)) {
        BoxWithConstraints(Modifier.fillMaxSize()) {
            val wide = maxWidth >= 760.dp
            LaunchedEffect(wide, pickedPageSize) {
                val size = if (wide) CatalogPaging.pageSize else CatalogPaging.phonePageSize
                if (!pickedPageSize && size != pageSize) resize(size)
            }
            Column(Modifier.fillMaxSize().padding(vertical = 20.dp), verticalArrangement = Arrangement.spacedBy(20.dp)) {
                Header(state, inset, showInstalled) { showCredentials = true }
                SearchBox(search, ::changeSearch, inset, ::submit) { changeSearch(""); submit() }
                if (wide) Row(Modifier.weight(1f).padding(horizontal = 44.dp), horizontalArrangement = Arrangement.spacedBy(20.dp)) {
                    ExtensionRail(section) { section = it }
                    Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        SourceLine()
                        Pins()
                        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            Lists()
                            Pagers()
                        }
                        Foot()
                    }
                } else Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).then(inset), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        TabChoice(section, ExtensionSection.entries.map { it to it.title }, { section = it })
                        SourceLine()
                    }
                    Pins()
                    Lists()
                    Pagers()
                    Foot()
                }
            }
        }
        // The sheet's own close, over the page.
        Box(Modifier.align(Alignment.TopEnd).padding(8.dp)) { IconButton("Close", "xmark", action = dismiss) }
    }
    CodyncSheet(installing, { installing = null }) { InstallConnectorSheet(it, state, store) { submit() } }
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
                Text(tr("{count} installed").replace("{count}", "$count"), style = Type.body, color = Palette.secondary)
                Text(tr("Connectors & skills"), style = Type.caption, color = Palette.secondary)
            }
            Symbol("chevron.right", size = 13.dp, tint = Palette.tertiary)
        }
    }
}

@Composable private fun SearchBox(text: String, onChange: (String) -> Unit, modifier: Modifier, submit: () -> Unit, clear: () -> Unit) {
    val clearSearch = tr("Clear search")
    Row(modifier.fillMaxWidth().height(38.dp).clip(Capsule()).background(Palette.bubbleAgent).padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Symbol("magnifyingglass", size = 18.dp, tint = Palette.secondary)
        PlainTextField(text, onChange, Modifier.weight(1f), "Search agents, connectors and skills", singleLine = true,
            imeAction = ImeAction.Search, actions = KeyboardActions(onSearch = { submit() }))
        if (text.isNotEmpty()) Box(Modifier.plainButton(action = clear).semantics { contentDescription = clearSearch }) {
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
                Text(tr("Node.js isn't on {name}").replace("{name}", state.hostName), style = Type.subheadline.semibold, color = Palette.text)
                Text(tr("Some agents need it to run. Codync can add a copy just for them."), style = Type.caption, color = Palette.secondary)
            }
            Text(tr("Add Node.js"), Modifier.clip(Capsule()).background(Palette.accentFill).plainButton(action = add).padding(horizontal = 14.dp, vertical = 7.dp),
                style = Type.subheadline.semibold, color = Palette.onAccent)
        }
    }
}
