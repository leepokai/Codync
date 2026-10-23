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
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
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
import androidx.compose.ui.layout.layout
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.text.style.TextOverflow
import com.codync.android.core.AgentBackend
import com.codync.android.core.CatalogPage
import com.codync.android.core.MarketConnector
import com.codync.android.core.MarketSkill
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/**
 * Extensions (iOS `MarketplaceView`): one page of agents, connectors, apps and skills for the bots on
 * this computer. Installing happens on the computer; each bot turns connectors and skills on in its settings.
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
    // Bumped on each submitted search, so every section searches again.
    var connectors by remember { mutableStateOf(CatalogPage<MarketConnector>()) }
    var connectorRequest by remember { mutableStateOf("") }
    var connectorQuery by remember { mutableStateOf("") }
    // The registry page after the ones shown; null at the end.
    var connectorCursor by remember { mutableStateOf<String?>(null) }
    var loadingConnectors by remember { mutableStateOf(true) }
    var loadingMore by remember { mutableStateOf(false) }
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
    // Agents shown before "Load more agents".
    var agentLimit by remember { mutableIntStateOf(10) }
    val query = search.trim()
    suspend fun loadConnectors() {
        val request = UUID.randomUUID().toString()
        connectorRequest = request; connectorQuery = query; loadingConnectors = true; loadingMore = false
        try {
            val page = store.marketConnectors(query, null)
            if (connectorRequest != request) return
            connectors = CatalogPage<MarketConnector>().append(page.items) { it.name }; connectorCursor = page.nextCursor?.takeIf(String::isNotBlank); error = null
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) {
            if (connectorRequest != request) return
            connectors = CatalogPage(); connectorCursor = null; error = failure.message ?: "Couldn't load connectors."
        } finally { if (connectorRequest == request) loadingConnectors = false }
    }
    fun loadMoreConnectors() {
        if (loadingMore || loadingConnectors) return
        if (connectors.hasHidden) { connectors = connectors.reveal(); return }
        val cursor = connectorCursor ?: return
        val request = connectorRequest
        loadingMore = true
        scope.launch {
            try {
                val page = store.marketConnectors(connectorQuery, cursor)
                if (connectorRequest != request) return@launch
                connectors = connectors.append(page.items) { it.name }.reveal()
                connectorCursor = page.nextCursor?.takeIf { it.isNotBlank() && it != cursor }; error = null
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { if (connectorRequest == request) error = failure.message ?: "Couldn't load more connectors." }
            finally { if (connectorRequest == request) loadingMore = false }
        }
    }
    fun submit() { agentLimit = 10; scope.launch { loadConnectors() } }
    LaunchedEffect(Unit) { store.refreshPlugins(); store.refreshBackends() }
    LaunchedEffect(Unit) { loadConnectors() }
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
    Box(Modifier.fillMaxSize().background(Palette.background)) {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(vertical = 20.dp), verticalArrangement = Arrangement.spacedBy(28.dp)) {
            Header(state, inset, showInstalled)
            SearchBox(search, { search = it }, inset, ::submit) { search = ""; submit() }
            if (agents.isNotEmpty()) Box(inset) {
                MarketSection(if (query.isEmpty()) "Agents" else tr("Agents matching “{query}”").replace("{query}", query)) {
                    if (backends.any { it.needsNode }) NodeNotice(state) { addingNode = true }
                    // The row runs edge to edge, past the page's margins.
                    LazyRow(Modifier.bleed(24.dp), contentPadding = PaddingValues(horizontal = 24.dp),
                        horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                        items(agents.take(agentLimit), key = { it.id }) { backend -> AgentCard(backend) { agent = backend } }
                    }
                    if (agents.size > agentLimit) Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                        SecondaryButton("Load more agents") { agentLimit += 10 }
                    }
                }
            }
            Box(inset) {
                MarketSection("Connectors") {
                    when {
                        loadingConnectors -> SkeletonRows()
                        connectors.items.isEmpty() -> Text(if (query.isEmpty()) tr("Couldn't load connectors.") else tr("No connectors match “{query}”.").replace("{query}", query), style = Type.body, color = Palette.secondary)
                        else -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            for (c in connectors.visible) MarketRow(c.title, c.description ?: c.name, c.installed,
                                icon = { ServiceLogo(c.website, c.title, c.name) }) { installing = c }
                            if (connectors.hasHidden || connectorCursor != null) Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                                SecondaryButton(if (loadingMore) "Loading…" else "Load more connectors", enabled = !loadingMore, action = ::loadMoreConnectors)
                            }
                        }
                    }
                }
            }
            Box(inset) {
                MarketSection("Skills") {
                    if (loadingSkills) SkeletonRows()
                    else Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        for (s in shownSkills) {
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
            }
            Box(inset) {
                SecondaryButton({ showCredentials = true }) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Symbol("lock.shield", size = 18.dp, tint = Palette.text)
                        Text(tr("Credentials"))
                    }
                }
            }
            Box(inset) {
                MarketSection("Make your own") {
                    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        MarketRow("Custom connector", "Any MCP server: a command or a URL", added = false, addLabel = "New",
                            icon = { TileIcon("point.3.connected.trianglepath.dotted") }) { addingConnector = true }
                        MarketRow("Your own skill", "Write instructions a bot can follow", added = false, addLabel = "New",
                            icon = { TileIcon("square.and.pencil") }) { writingSkill = true }
                    }
                }
            }
            error?.let { Text(tr(it), inset, style = Type.footnote, color = Palette.danger) }
            Text(tr("Connectors come from the official MCP Registry, skills from Anthropic, agents from the ACP registry. Everything installs on {name}.").replace("{name}", state.hostName),
                inset, style = Type.footnote, color = Palette.tertiary)
        }
        // The sheet's own close, over the page.
        Box(Modifier.align(Alignment.TopEnd).padding(8.dp)) { IconButton("Close", "xmark", action = dismiss) }
    }
    CodyncSheet(installing, { installing = null }) { InstallConnectorSheet(it, state, store) { scope.launch { loadConnectors() } } }
    CodyncSheet(showCredentials, { showCredentials = false }) { CredentialsView(state, store) }
    CodyncSheet(addingConnector, { addingConnector = false }) { CustomConnectorSheet(state, store) }
    CodyncSheet(viewingSkill, { viewingSkill = null }) { SkillSheet(it, state, store) }
    CodyncSheet(writingSkill, { writingSkill = false }) { NewSkillSheet(store) }
    CodyncSheet(agent, { agent = null }) { AgentSheet(it, state, store) }
    CodyncSheet(addingNode, { addingNode = false }) { SetupTerminal(null, "node", null, state, store) { addingNode = false } }
}

/** Widens a child past its parent's margins by `amount` on each side. */
private fun Modifier.bleed(amount: Dp) = layout { measurable, constraints ->
    val extra = amount.roundToPx()
    val width = constraints.maxWidth + 2 * extra
    val placeable = measurable.measure(constraints.copy(minWidth = width, maxWidth = width))
    layout(constraints.maxWidth, placeable.height) { placeable.place(-extra, 0) }
}

/** The computer it all lives on, and the way to what's installed. */
@Composable private fun Header(state: AppState, modifier: Modifier, showInstalled: () -> Unit) {
    val connectors = state.plugins?.connectors.orEmpty()
    val count = connectors.size + state.plugins?.skills.orEmpty().size
    Column(modifier.padding(top = 28.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            ComputerBadge(state.hello?.text("device"), 32.dp, state.computers.firstOrNull()?.color)
            Column {
                Text(tr("Extensions"), style = Type.title2.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(state.hostName, style = Type.subheadline, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
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
