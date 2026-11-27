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

/**
 * The apps given MCP access on a computer: each one's level and expiry, and Revoke (iOS `McpAccessSheet`). Apps are
 * added and their access set on the computer itself (the host takes those calls from loopback only).
 */
@Composable internal fun McpAccessSheet(store: AppStore) {
    val state by store.state.collectAsState()
    var revoking by remember { mutableStateOf<McpApp?>(null) }
    LaunchedEffect(Unit) { store.loadMcpApps() }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader({
            Text(tr("MCP access"), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        })
        CardForm {
            CardSection(footer = "Approvals are never answered through MCP: every request still comes to your devices.",
                modifier = Modifier.animateContentSize(Motion.layoutSize)) {
                val apps = state.mcpApps
                when {
                    // A first read that failed says why here; a later failure keeps the list and shows as the store's error.
                    apps == null -> row {
                        val error = state.error
                        if (error != null) Text(error, style = Type.subheadline, color = Palette.danger)
                        else Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { Spinner() }
                    }
                    apps.isEmpty() -> row { Text(tr("No apps yet. They're added on the computer."), style = Type.subheadline, color = Palette.secondary) }
                    else -> for (app in apps) row { McpAppRow(app, state.revokingMcpApp(app.id)) { revoking = app } }
                }
            }
        }
    }
    val target = revoking
    CodyncDialog(fill(tr("Revoke {name}?"), mapOf("name" to (target?.label ?: ""))), target != null, { revoking = null },
        "Its token stops working right away.") {
        listOfNotNull(target?.let { app -> DialogAction("Revoke", destructive = true) { store.revokeMcpApp(app.id) } })
    }
}

/** [busy]: its revoke is on its way, so the button waits. */
@Composable private fun McpAppRow(app: McpApp, busy: Boolean, revoke: () -> Unit) {
    val left = app.daysLeft()
    val until = if (left == 0) tr("Expired") else fill(tr("Expires in {count} days"), mapOf("count" to "$left"))
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(app.label, style = Type.body, color = Palette.text)
            Text("${tr(app.levelName)} · $until", style = Type.footnote, color = Palette.secondary)
        }
        IconButton("Revoke access", "xmark.circle", enabled = !busy, action = revoke)
    }
}
