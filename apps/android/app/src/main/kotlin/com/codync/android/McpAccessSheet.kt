package com.codync.android

import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.*
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.McpApp
import com.codync.android.core.fill
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/**
 * The apps given MCP access on a computer: each one's level and expiry, and Revoke (iOS `McpAccessSheet`). Apps are
 * added and their access set on the computer itself (the host takes those calls from loopback only).
 */
@Composable internal fun McpAccessSheet(store: AppStore) {
    var apps by remember { mutableStateOf<List<McpApp>?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    var revoking by remember { mutableStateOf<McpApp?>(null) }
    val scope = rememberCoroutineScope()
    suspend fun load() {
        try { apps = McpApp.active(store.hostQuery("mcpClients", body())); error = null }
        catch (e: CancellationException) { throw e }
        catch (e: Exception) { error = e.message ?: e.toString() }
    }
    LaunchedEffect(Unit) { load() }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader({
            Text(tr("MCP access"), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        })
        CardForm {
            CardSection(footer = "Approvals are never answered through MCP: every request still comes to your devices.",
                modifier = Modifier.animateContentSize(Motion.layoutSize)) {
                error?.let { row { Text(it, style = Type.subheadline, color = Palette.danger) } }
                val shown = apps
                when {
                    shown == null -> if (error == null) row { Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { Spinner() } }
                    shown.isEmpty() -> row { Text(tr("No apps yet. They're added on the computer."), style = Type.subheadline, color = Palette.secondary) }
                    else -> for (app in shown) row { McpAppRow(app) { revoking = app } }
                }
            }
        }
    }
    val target = revoking
    CodyncDialog(fill(tr("Revoke {name}?"), mapOf("name" to (target?.label ?: ""))), target != null, { revoking = null },
        "Its token stops working right away.") {
        listOfNotNull(target?.let { app ->
            DialogAction("Revoke", destructive = true) {
                scope.launch {
                    try { store.hostQuery("mcpClientRevoke", body("id" to app.id)); load() }
                    catch (e: CancellationException) { throw e }
                    catch (e: Exception) { error = e.message ?: e.toString() }
                }
            }
        })
    }
}

@Composable private fun McpAppRow(app: McpApp, revoke: () -> Unit) {
    val left = app.daysLeft()
    val until = if (left == 0) tr("Expired") else fill(tr("Expires in {count} days"), mapOf("count" to "$left"))
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(app.label, style = Type.body, color = Palette.text)
            Text("${tr(app.levelName)} · $until", style = Type.footnote, color = Palette.secondary)
        }
        IconButton("Revoke access", "xmark.circle", action = revoke)
    }
}
