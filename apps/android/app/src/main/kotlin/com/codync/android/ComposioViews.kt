package com.codync.android

import androidx.activity.compose.LocalActivity
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.unit.dp
import androidx.compose.ui.text.style.TextOverflow
import com.codync.android.core.CatalogPage
import com.codync.android.core.ComposioApp
import com.codync.android.core.ComposioStatus
import com.codync.android.design.*
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

// Apps through Composio (host/src/market/composio.rs): set up a key once, connect apps with
// Composio's hosted sign-in, then turn them on per bot like any other connector.

/** The Extensions page's "Apps" section (iOS `ComposioSection`). */
@Composable internal fun ComposioSection(query: String, searchToken: Int, state: AppState, store: AppStore) {
    var status by remember { mutableStateOf<ComposioStatus?>(null) }
    var apps by remember { mutableStateOf(CatalogPage<ComposioApp>()) }
    var loading by remember { mutableStateOf(true) }
    var error by remember { mutableStateOf<String?>(null) }
    var settingUp by remember { mutableStateOf(false) }
    var connecting by remember { mutableStateOf<ComposioApp?>(null) }
    var reload by remember { mutableIntStateOf(0) }
    LaunchedEffect(searchToken, reload) {
        loading = true
        try {
            val answer = store.composioStatus()
            val page = if (answer.configured) store.composioApps(query, null).items else emptyList()
            status = answer; apps = CatalogPage<ComposioApp>().append(page) { it.slug }; error = null
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "Couldn't load apps." }
        finally { loading = false }
    }
    MarketSection("Apps") {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            when {
                status?.configured != true -> MarketRow("Composio", "Gmail, Slack, GitHub, Notion and 1,000 more apps", added = false,
                    busy = status == null && loading, addLabel = "Set up", icon = { TileIcon("square.grid.3x3") }) { settingUp = true }
                loading -> SkeletonRows()
                apps.items.isEmpty() -> Text(if (query.isEmpty()) "Couldn't load apps." else "No apps match “$query”.", style = Type.body, color = Palette.secondary)
                else -> Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                    for (app in apps.visible) MarketRow(app.name, app.description ?: app.slug, app.connected, addLabel = "Connect",
                        icon = { AppLogo(app.logo, app.name) }) { connecting = app }
                }
            }
            if (!loading && apps.hasHidden && status?.configured == true)
                Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { SecondaryButton("Load more apps") { apps = apps.reveal() } }
            error?.let { Text(it, style = Type.footnote, color = Palette.danger) }
            if (status?.configured == true) Row(Modifier.plainButton { settingUp = true }, verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Symbol("key", size = 14.dp, tint = Palette.tertiary)
                Text("Composio settings", style = Type.footnote, color = Palette.tertiary)
            }
        }
    }
    CodyncSheet(settingUp, { settingUp = false }) { ComposioKeySheet(status, state, store) { status = it; reload++ } }
    CodyncSheet(connecting, { connecting = null }) { ComposioConnectSheet(it, state, store) { reload++; store.refreshPlugins() } }
}

/** Paste (or remove) the Composio API key; the host checks it with Composio first (iOS `ComposioKeySheet`). */
@Composable internal fun ComposioKeySheet(status: ComposioStatus?, state: AppState, store: AppStore, done: (ComposioStatus) -> Unit) {
    val dismiss = LocalDismissModal.current
    val scope = rememberCoroutineScope()
    var key by remember { mutableStateOf("") }
    var saving by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    fun save(value: String) {
        saving = true; error = null
        scope.launch {
            try { done(store.composioKey(value)); dismiss() }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message ?: "The key wasn't confirmed." }
            finally { saving = false }
        }
    }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("Composio") {
            if (saving) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
            else IconButton("Save", "checkmark", enabled = key.isNotBlank()) { save(key.trim()) }
        }
        CardForm(Modifier.weight(1f)) {
            CardSection("Composio API key", "Composio signs you in to apps like Gmail and Slack and runs their tools for your bots. The key stays on ${state.hostName}.") {
                row { PlainInput(key, { key = it }, if (status?.configured == true) "Saved (paste to replace)" else "ak_…", secret = true) }
                row { WebLink("Get a key from Composio", status?.keyUrl ?: "https://platform.composio.dev", Type.footnote) }
            }
            if (status?.configured == true) CardSection(footer = "Bots lose the apps you connected until you add a key again.") {
                row { LinkButton("Remove key", enabled = !saving, color = Palette.danger) { save("") } }
            }
            error?.let { CardSection { row { ErrorRow(it) } } }
        }
    }
}

/**
 * Connects one app (iOS `ComposioConnectSheet`): Composio's sign-in page, opened here and
 * finished anywhere, or a form for apps that use a key. A chat's request finishes once it's connected.
 */
@Composable internal fun ComposioConnectSheet(app: ComposioApp, state: AppState, store: AppStore, requestId: String? = null, done: () -> Unit = {}) {
    val dismiss = LocalDismissModal.current
    val uri = LocalUriHandler.current
    val activity = LocalActivity.current
    val owner = rememberSaveable { UUID.randomUUID().toString() }
    var connectionId by rememberSaveable { mutableStateOf<String?>(null) }
    val session = remember(owner) { store.appConnection(owner, app.slug, connectionId) }
    val status by session.state.collectAsState()
    var opened by rememberSaveable { mutableStateOf(false) }
    var finished by rememberSaveable { mutableStateOf(false) }
    var values by remember { mutableStateOf(emptyMap<String, String>()) }
    var error by remember { mutableStateOf<String?>(null) }
    fun open(url: String) { if (runCatching { uri.openUri(url) }.isFailure) error = "Couldn't open the sign-in browser." }
    DisposableEffect(session) {
        onDispose {
            // Rotation keeps the attempt; leaving ends it.
            if (activity?.isChangingConfigurations != true) store.closeAppConnection(owner)
            if (finished) done()
        }
    }
    LaunchedEffect(status.connectionId) { status.connectionId?.let { connectionId = it } }
    LaunchedEffect(status.plan?.url) { status.plan?.url?.let { if (!opened) { opened = true; open(it) } } }
    LaunchedEffect(status.connected) {
        if (!status.connected || finished) return@LaunchedEffect
        try { requestId?.let { store.finishConnectionRequest(it, connectorId = "composio-${app.slug}") }; values = emptyMap(); finished = true }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "The app is connected, but the request wasn't confirmed." }
    }
    val plan = status.plan
    val connected = status.connected && finished
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader("Connect ${app.name}") {
            if (connected) IconButton("Done", "checkmark", action = dismiss)
            else if (plan?.status == "needsFields") {
                if (status.saving) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
                else IconButton("Connect", "checkmark", enabled = plan.complete(values)) { session.submit(values) }
            }
        }
        CardForm(Modifier.weight(1f)) {
            CardSection {
                row {
                    Row(Modifier.padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                        AppLogo(app.logo, app.name, 52.dp)
                        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Text(app.name, style = Type.title3.semibold, color = Palette.text)
                            app.description?.let { Text(it, style = Type.subheadline, color = Palette.secondary, maxLines = 3, overflow = TextOverflow.Ellipsis) }
                        }
                    }
                }
            }
            when {
                connected -> CardSection(footer = "Turn ${app.name} on for a bot in the bot's settings, under Connectors.") {
                    row { IconLabel("Connected", "checkmark.circle.fill", Palette.added, style = Type.body) }
                }
                plan?.status == "needsFields" -> CardSection(footer = "Sent to Composio, which keeps it for ${app.name}.") {
                    for (field in plan.fields.orEmpty()) row {
                        NotedInput(values[field.name].orEmpty(), { values = values + (field.name to it) }, field.label,
                            field.description?.let { it + if (field.required) "" else " (optional)" }, secret = field.secret)
                    }
                }
                plan?.status == "redirect" || status.connectionId != null -> CardSection(footer = "Sign in on the page that opened. This updates by itself when you're done.") {
                    plan?.url?.let { url ->
                        row {
                            SecondaryButton({ open(url) }) {
                                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                    Symbol("safari", size = 18.dp, tint = Palette.text)
                                    Text("Open the sign-in page")
                                }
                            }
                        }
                    }
                    row { Waiting("Waiting for you to finish signing in…") }
                }
                status.error == null && error == null -> CardSection { row { Waiting("Asking Composio…") } }
            }
            (error ?: status.error)?.let { CardSection { row { ErrorRow(it) } } }
        }
    }
}
