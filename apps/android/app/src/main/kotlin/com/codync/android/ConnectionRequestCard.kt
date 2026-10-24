package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.AgentBackend
import com.codync.android.core.ComposioApp
import com.codync.android.core.ConnectionRequest
import com.codync.android.core.Entry
import com.codync.android.core.MarketConnector
import com.codync.android.core.WireJson
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import kotlinx.serialization.json.decodeFromJsonElement

/**
 * A bot asking for a connection in the chat (iOS `ConnectionRequestCard`). Credentials go through
 * the setup API; only the request's state is a chat entry. Apps and registry connectors open their
 * sheets over the chat (`openApp` / `install`); an agent's sign-in opens its sheet here.
 */
@Composable internal fun ConnectionRequestCard(entry: Entry, state: AppState, store: AppStore,
    openApp: (String, ComposioApp) -> Unit, install: (String, MarketConnector) -> Unit) {
    val request = entry.data["connectionRequest"]?.let { runCatching { WireJson.decodeFromJsonElement<ConnectionRequest>(it) }.getOrNull() }
    if (request == null || request.botId != entry.botId && state.bots.none { it.id == request.botId }) {
        Text("This connection request needs a newer app.", style = Type.body, color = Palette.secondary); return
    }
    val scope = rememberCoroutineScope()
    var value by remember(entry.id) { mutableStateOf("") }
    var username by remember(entry.id) { mutableStateOf("") }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    var agent by remember { mutableStateOf<AgentBackend?>(null) }
    val pending = request.status == "pending"
    val working = busy || "connection:${entry.id}" in state.busy
    fun connect() {
        busy = true; error = null
        scope.launch {
            try {
                when {
                    // The host completes the card and resends the messages once the sheet finds it signed in.
                    request.kind == "agent" -> agent = agentBackends(state).firstOrNull { it.id == request.backend }
                        ?: throw IOException("Open ${request.title} in Extensions to sign in.")
                    request.kind == "app" -> openApp(entry.id, ComposioApp(requireNotNull(request.toolkit), request.title))
                    request.kind == "login" -> { store.finishConnectionRequest(entry.id, value = value, username = username); value = "" }
                    request.kind == "secret" -> { store.finishConnectionRequest(entry.id, value = value); value = "" }
                    request.connectorId != null -> {
                        val id = requireNotNull(request.connectorId)
                        // The store finishes the request itself once a sign-in comes back.
                        if (store.installedConnectors().firstOrNull { it.id == id }?.needsSignIn == true) {
                            if (!store.signInConnector(id, entry.id)) throw IOException("Sign-in wasn't confirmed. Connect again to retry.")
                        } else store.finishConnectionRequest(entry.id, connectorId = id)
                    }
                    request.registryName != null -> install(entry.id, store.connectorInfo(requireNotNull(request.registryName)))
                }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message ?: "Connection wasn't confirmed. Check its setup and try again." }
            finally { busy = false }
        }
    }
    // A card, not a banner: full width on a phone, capped on wider screens; no fill, so each field gets one.
    Column(Modifier.testTag("connection:${entry.id}").widthIn(max = 400.dp).fillMaxWidth().border(1.dp, Palette.border, RoundedRectangle(20.dp))
        .padding(horizontal = 16.dp, vertical = 14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(14.dp)) {
            RequestLogo(request, state)
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Text(request.title, style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (entry.text.isNotBlank()) Text(entry.text, style = Type.subheadline, color = Palette.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
            if (pending) Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                if (working) Spinner()
                PrimaryButton(when (request.kind) { "agent" -> "Sign in"; "login" -> "Save"; else -> "Connect" },
                    enabled = !working && request.canConnect && state.actualConnection is com.codync.android.core.LinkState.Ready &&
                        (request.kind !in listOf("secret", "login") || value.isNotEmpty()), action = ::connect)
            } else Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                if (request.status == "ready") Symbol("checkmark", size = 14.dp, tint = Palette.secondary)
                Text(if (request.status != "ready") "Cancelled" else when (request.kind) { "agent" -> "Signed in"; "login" -> "Saved"; else -> "Connected" },
                    style = Type.subheadline, color = Palette.secondary)
            }
        }
        if (pending && request.kind == "login") {
            Box(Modifier.pill(Palette.bubbleAgent)) { PlainInput(username, { username = it }, "Username or email") }
            Box(Modifier.pill(Palette.bubbleAgent)) { PlainInput(value, { value = it }, "Password or op:// reference", secret = true) }
            Text("Saved securely on this computer. The bot types it into the sign-in page but never sees it.", style = Type.caption, color = Palette.secondary)
        } else if (pending && request.kind == "secret") {
            Text("${request.field ?: "Credential"} · ${request.location.orEmpty()}", style = Type.caption, color = Palette.secondary)
            Box(Modifier.pill(Palette.bubbleAgent)) { PlainInput(value, { value = it }, "Credential or op:// reference", secret = true) }
            Text("Saved securely on this computer. Never added to the conversation.", style = Type.caption, color = Palette.secondary)
        }
        error?.let { Text(it, style = Type.footnote, color = Palette.danger) }
    }
    CodyncSheet(agent, { agent = null }) { AgentSheet(it, state, store, recheck = true) }
}

/** The agent, website or service the request is for. */
@Composable private fun RequestLogo(request: ConnectionRequest, state: AppState) {
    when (request.kind) {
        "agent" -> Box(Modifier.size(46.dp).clip(RoundedRectangle(12.dp)).background(Palette.bubbleAgent), contentAlignment = Alignment.Center) {
            AgentIcon(request.backend?.let { agentRegistry(state, it) }, 26.dp)
        }
        "app" -> AppLogo(request.logo, request.title)
        // A login's site is a domain, or an app's name (no logo to find for that).
        "login" -> ServiceLogo(null, request.title, site = request.site?.takeIf { '.' in it })
        else -> ServiceLogo(null, request.title, request.registryName)
    }
}
