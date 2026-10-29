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
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.AgentAuth
import com.codync.android.core.AgentAuthMethod
import com.codync.android.core.AgentBackend
import com.codync.android.core.LinkState
import com.codync.android.core.WireJson
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import kotlinx.serialization.json.decodeFromJsonElement

/** A step inside the agent's sheet: a terminal for install / Node.js / sign-in, or its keys. */
private data class SetupRoute(val step: String, val method: AgentAuthMethod? = null)

/** The computer's agents, live: an install or sign-in updates the host's list. */
internal fun agentBackends(state: AppState): List<AgentBackend> =
    state.hello?.get("backends")?.let { runCatching { WireJson.decodeFromJsonElement<List<AgentBackend>>(it) }.getOrNull() }.orEmpty()

/** The ACP registry id an agent's logo is under (`AgentIcon`). */
internal fun agentRegistry(state: AppState, agent: String) = agentBackends(state).firstOrNull { it.id == agent }?.registry ?: agent

/**
 * Getting an agent ready on the computer (iOS `AgentSheet`): install it, add Node.js when it runs
 * on Node and the computer has none, then sign in. `recheck` (a chat's sign-in request) asks the
 * agent again however signed in it looked.
 */
@Composable internal fun AgentSheet(initial: AgentBackend, state: AppState, store: AppStore, recheck: Boolean = false) {
    val scope = rememberCoroutineScope()
    val backend = agentBackends(state).firstOrNull { it.id == initial.id } ?: initial
    var route by remember { mutableStateOf<SetupRoute?>(null) }
    var auth by remember { mutableStateOf<AgentAuth?>(null) }
    var checking by remember { mutableStateOf(false) }
    var authenticating by remember { mutableStateOf<String?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    fun check() {
        if (checking) return
        checking = true
        scope.launch {
            try { auth = store.agentAuth(backend.id); error = null }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message ?: "Couldn't check this agent." }
            finally { checking = false }
        }
    }
    // A known "signed in" needs no agent start; one set up asks the agent (which can't start without its Node.js).
    // Opening never downloads one: that waits for Set up.
    LaunchedEffect(Unit) { if ((recheck || (setUp(backend, auth) && backend.signedIn != true)) && !backend.needsNode) check() }
    val back = { route = null; check() }
    AnimatedContent(route, transitionSpec = {
        val forward = targetState != null
        (slideInHorizontally(Motion.layoutOffset) { if (forward) it else -it } + fadeIn(Motion.fade())) togetherWith
            (slideOutHorizontally(Motion.layoutOffset) { if (forward) -it else it } + fadeOut(Motion.fade()))
    }, label = "Agent setup") { shown ->
        when {
            shown == null -> Column(Modifier.fillMaxSize().background(Palette.background)) {
                ModalHeader(backend.name)
                Overview(backend, auth, checking, authenticating, error, state, store, { route = it }, ::check) { method ->
                    authenticating = method.id
                    scope.launch {
                        try { auth = store.agentAuthenticate(backend.id, method.id); error = null }
                        catch (cancelled: CancellationException) { throw cancelled }
                        catch (failure: Exception) { error = failure.message ?: "Sign-in didn't finish." }
                        finally { authenticating = null }
                    }
                }
            }
            shown.step == "keys" -> AgentKeysForm(backend, requireNotNull(shown.method), auth?.savedEnv.orEmpty(), state, store, back) { auth = it }
            else -> SetupTerminal(backend.takeUnless { shown.step == "node" }, shown.step, shown.method, state, store, back)
        }
    }
}

@Composable private fun Overview(backend: AgentBackend, auth: AgentAuth?, checking: Boolean, authenticating: String?, error: String?, state: AppState,
    store: AppStore, go: (SetupRoute) -> Unit, check: () -> Unit, authenticate: (AgentAuthMethod) -> Unit) {
    val host = state.hostName
    val signedIn = auth?.signedIn ?: backend.signedIn
    val curated = backend.curated
    val setUp = setUp(backend, auth)
    CardForm {
        CardSection {
            row {
                Row(Modifier.padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
                    Box(Modifier.size(52.dp).clip(RoundedRectangle(14.dp)).background(Palette.bubbleAgent), contentAlignment = Alignment.Center) {
                        AgentIcon(backend.registry ?: backend.id, 30.dp)
                    }
                    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(backend.name, style = Type.title3.semibold, color = Palette.text)
                        backend.description?.takeIf(String::isNotEmpty)?.let {
                            Text(tr(it), style = Type.subheadline, color = Palette.secondary, maxLines = 3, overflow = TextOverflow.Ellipsis)
                        }
                    }
                }
            }
        }
        if (curated) CardSection {
            row {
                SetupStepRow(1, "Install", if (backend.installed) "Installed on $host." else if (backend.canInstall) "Runs the official installer on $host." else backend.installHint,
                    backend.installed, ("Install" to { go(SetupRoute("install")) }).takeIf { !backend.installed && backend.canInstall })
            }
        }
        if (backend.node != null) CardSection {
            row {
                SetupStepRow(if (curated) 2 else 1, "Node.js", if (backend.needsNode) "${backend.name} runs on Node.js. Codync adds its own copy." else "Added on $host.",
                    !backend.needsNode, ("Add" to { go(SetupRoute("node")) }).takeIf { backend.needsNode })
            }
        }
        val agentMethods = auth?.methods.orEmpty().any { it.kind == "agent" || it.kind == null }
        CardSection(footer = if (signedIn != true && agentMethods) "Browser sign-ins open on $host itself. Away from it? Use Remote screen to finish there." else null) {
            row {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Box(Modifier.weight(1f)) {
                        SetupStepRow(1 + (if (curated) 1 else 0) + (if (backend.node != null) 1 else 0), "Sign in",
                            statusText(backend, setUp, auth, signedIn, checking, host), signedIn == true, null)
                    }
                    if (checking) ThinkingOrb(ThinkingOrbState.Connecting, 16.dp, Palette.secondary) else IconButton("Check again", "arrow.clockwise", action = check)
                }
            }
            val detail = auth?.detail
            if (signedIn != true && !detail.isNullOrEmpty()) row { SelectionContainer { Text(detail, style = Type.footnote, color = Palette.secondary) } }
            // A registry agent downloads on its first start; an app's own CLI comes from Install above.
            if (!setUp && !curated && !checking && !backend.needsNode) row {
                OptionRow("Set up", "Downloads ${backend.name} on $host if it needs to, then checks its sign-in.", "arrow.down.circle",
                    enabled = authenticating == null, action = check)
            }
            // Some CLIs drop the current sign-in the moment a new one starts: no options once signed in.
            if (signedIn != true && !checking) {
                if (auth?.login == true) row {
                    OptionRow("Sign in", "In a terminal on $host. Links open here.", "terminal", enabled = authenticating == null) { go(SetupRoute("login")) }
                }
                for (method in auth?.methods.orEmpty()) row {
                    when (method.kind) {
                        "terminal" -> OptionRow(method.name, method.description ?: "In a terminal on $host.", "terminal", enabled = authenticating == null) {
                            go(SetupRoute("login", method))
                        }
                        "envVar" -> OptionRow(method.name, method.description ?: "Saved on $host only.", "key", enabled = authenticating == null) {
                            go(SetupRoute("keys", method))
                        }
                        "agent", null -> OptionRow(method.name, if (authenticating == method.id) "Finish signing in in the browser on $host…"
                            else method.description ?: "Opens a browser on $host.", "safari", busy = authenticating == method.id, enabled = authenticating == null) { authenticate(method) }
                        else -> OptionRow(method.name, "This sign-in needs a newer app. Sign in on $host for now.", "exclamationmark.circle", enabled = false) {}
                    }
                }
            }
        }
        AgentBrings(backend.id, store, state.connection is LinkState.Ready)
        error?.let { CardSection { row { SelectionContainer { ErrorRow(it) } } } }
    }
}

/** It has run on this computer: its CLI is there, or an earlier check started it. Until then, a check can download it. */
private fun setUp(backend: AgentBackend, auth: AgentAuth?) = backend.installed || backend.signedIn != null || auth != null

private fun statusText(backend: AgentBackend, setUp: Boolean, auth: AgentAuth?, signedIn: Boolean?, checking: Boolean, host: String) = when {
    checking -> if (auth == null) "Checking with ${backend.name}… The first check can download it." else "Checking…"
    signedIn == true -> "Signed in."
    backend.needsNode -> "Add Node.js first."
    !setUp -> if (backend.curated) "Install it first." else "Not set up on $host yet."
    signedIn == false -> "Not signed in yet."
    else -> "Couldn't tell. Skip this if you've already signed in on $host."
}

@Composable private fun OptionRow(title: String, detail: String, icon: String, busy: Boolean = false, enabled: Boolean = true, action: () -> Unit) {
    Row(Modifier.fillMaxWidth().plainButton(enabled = enabled, action = action), verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.width(26.dp), contentAlignment = Alignment.Center) { Symbol(icon, size = 20.dp, tint = Palette.secondary) }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(tr(title), style = Type.body.medium, color = Palette.text)
            Text(tr(detail), style = Type.subheadline, color = Palette.secondary, maxLines = 3, overflow = TextOverflow.Ellipsis)
        }
        if (busy) ThinkingOrb(size = 16.dp, color = Palette.secondary) else Symbol("chevron.right", size = 13.dp, tint = Palette.tertiary)
    }
}

/** A numbered setup step: done shows a check; an action sits on the right. */
@Composable private fun SetupStepRow(number: Int, title: String, detail: String, done: Boolean, action: Pair<String, () -> Unit>?) {
    val doneLabel = tr("Done")
    Row(Modifier.padding(vertical = 4.dp).semantics(mergeDescendants = true) { stateDescription = if (done) doneLabel else "" },
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.size(26.dp).clip(Capsule()).background(if (done) Palette.accentFill else Palette.bubbleAgent), contentAlignment = Alignment.Center) {
            if (done) Symbol("checkmark", size = 13.dp, tint = Palette.onAccent)
            else Text("$number", style = Type.caption.semibold, color = Palette.secondary)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(tr(title), style = Type.body.medium, color = Palette.text)
            Text(tr(detail), style = Type.subheadline, color = Palette.secondary)
        }
        action?.let { (label, run) ->
            Text(tr(label), Modifier.clip(Capsule()).background(Palette.bubbleAgent).plainButton(action = run).padding(horizontal = 14.dp, vertical = 7.dp),
                style = Type.subheadline.semibold, color = Palette.text)
        }
    }
}

/** Keys an agent reads from its environment, kept on the computer (iOS `AgentKeysForm`). */
@Composable private fun AgentKeysForm(backend: AgentBackend, method: AgentAuthMethod, saved: List<String>, state: AppState, store: AppStore,
    back: () -> Unit, done: (AgentAuth) -> Unit) {
    val scope = rememberCoroutineScope()
    var values by remember { mutableStateOf(emptyMap<String, String>()) }
    var saving by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    fun save(vars: Map<String, String>) {
        saving = true; error = null
        scope.launch {
            try { done(store.setAgentEnv(backend.id, vars)); back() }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message ?: "Couldn't update these keys." }
            finally { saving = false }
        }
    }
    val savedHere = method.vars.any { it.name in saved }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ScreenHeader({ BackButton(back) }, { Text(tr(method.name), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis) }) {
            if (saving) Box(Modifier.size(36.dp), contentAlignment = Alignment.Center) { Spinner() }
            else IconButton("Save", "checkmark", enabled = method.complete(values, saved)) { save(values.filterValues(String::isNotEmpty)) }
        }
        CardForm(Modifier.weight(1f)) {
            Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
                CardSection(method.name) {
                    for (v in method.vars) row {
                        NotedInput(values[v.name].orEmpty(), { values = values + (v.name to it) }, if (v.name in saved) "Saved (type to replace)" else v.name,
                            tr(v.label) + if (v.optional) tr(" (optional)") else "", secret = v.secret)
                    }
                }
                Column(Modifier.padding(horizontal = 4.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    method.description?.let { Text(tr(it), style = Type.caption, color = Palette.tertiary) }
                    Text("Saved on ${state.hostName} only and given to ${backend.name} when it starts.", style = Type.caption, color = Palette.tertiary)
                    method.link?.let { WebLink("Get a key", it, Type.caption) }
                }
            }
            if (savedHere) CardSection {
                row { LinkButton("Remove saved keys", enabled = !saving, color = Palette.danger) { save(method.vars.associate { it.name to "" }) } }
            }
            error?.let { CardSection { row { ErrorRow(it) } } }
        }
    }
}
