package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.testTagsAsResourceId
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.NetworkComputer
import com.codync.android.core.NetworkInfo
import com.codync.android.core.NetworkRule
import com.codync.android.core.NetworkSession
import com.codync.android.core.NetworkView
import com.codync.android.core.fill
import com.codync.android.design.*
import com.kyant.shapes.Capsule

/**
 * The paired computer's networks, read-only (iOS `NetworkSheet`): in each, the computers with the bots each offers
 * and who may message the bots here; and whether cloud assistants may reach its bots. Changes are made on a computer.
 */
@Composable internal fun NetworkSheet(view: NetworkView, refresh: () -> Unit) {
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Network))
    // Presence as of now: the host asks the others when stale, and its event updates this.
    LaunchedEffect(Unit) { refresh() }
    var showOlder by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader({
            Symbol("codync.network", size = 24.dp, tint = Palette.text)
            Spacer(Modifier.width(8.dp))
            Text(tr("Network"), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Spacer(Modifier.width(8.dp))
            // The network is a beta feature.
            BetaTag()
        })
        CardForm {
            // One section per network; who may message the bots here only where it has rules.
            for (network in view.networks) {
                CardSection(network.name) {
                    for (computer in network.computers) row { NetworkComputerRow(computer) }
                }
                if (network.rules.isNotEmpty()) CardSection("Who may message bots here", network.rulesFooter) {
                    for (rule in network.rules) row { RuleRow(network, rule) }
                }
            }
            // The note that changes happen at a computer closes the networks, after the last one's rules.
            if (view.networks.isNotEmpty()) CardSection(footer = "Changes are made on a computer: in Codync's settings there, or with codync-host network.") {}
            if (view.sessions.isNotEmpty()) {
                // In the host's order: running first, then the most recent.
                val (older, recent) = view.sessions.partition { it.older }
                CardSection("Agents", "Bots and other agents message these sessions at their address; each reads its mail itself.") {
                    for (session in recent) row { NetworkSessionRow(session) }
                    if (older.isNotEmpty()) row {
                        Disclosure(showOlder, { showOlder = !showOlder }, Modifier.semantics { testTagsAsResourceId = true }.testTag("network.olderSessions"), {
                            Text(fill(tr("Older sessions: {count}"), "count" to older.size.toString()), style = Type.subheadline, color = Palette.secondary)
                        }) { for (session in older) NetworkSessionRow(session) }
                    }
                }
            }
            if (view.assistantsAvailable) CardSection(footer = "ChatGPT or Claude in your Codync account can message this computer's bots when it's on. Codync's cloud can read these calls.") {
                row { ValueRow("Cloud assistants", if (view.assistants) "On" else "Off") }
            }
        }
    }
}

/** A coding agent's session: whether it runs, its logo and title (else address), then its address, what it does and how it gets its mail. */
@Composable private fun NetworkSessionRow(session: NetworkSession) {
    val detail = listOfNotNull(session.title?.let { session.address }, session.stateTitle?.let { tr(it) }, session.tierTitle?.let { tr(it) })
        .joinToString(" · ")
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        // No state, no dot: unknown isn't offline.
        val dot = session.dotTitle?.let { tr(it) }
        Box(Modifier.padding(horizontal = 2.dp).size(8.dp).clip(Capsule())
            .then(if (session.state == null) Modifier else Modifier.background(if (session.runs) Palette.added else Palette.tertiary))
            .then(if (dot == null) Modifier else Modifier.semantics { contentDescription = dot }))
        AgentIcon(session.registry, 18.dp)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(session.title ?: session.address, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (detail.isNotEmpty()) Text(detail, style = Type.footnote, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

/** One rule, read-only: rules are changed on the computer. */
@Composable private fun RuleRow(network: NetworkInfo, rule: NetworkRule) {
    Text(network.describe(rule, translator()), style = Type.body, color = Palette.text)
}

/** One computer: its name and role, whether it answers, and the bots it offers. */
@Composable private fun NetworkComputerRow(computer: NetworkComputer) {
    val bots = computer.agents.joinToString(", ") { it.name }.ifEmpty { tr("No bots") }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.padding(horizontal = 2.dp).size(8.dp).clip(Capsule())
            .background(if (computer.me || computer.online) Palette.added else Palette.tertiary))
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(computer.name, Modifier.weight(1f, fill = false), style = Type.body.semibold, color = Palette.text,
                    maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(tr(computer.roleTitle), style = Type.caption, color = Palette.secondary)
            }
            Text("${tr(computer.presence)} · $bots", style = Type.footnote, color = Palette.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
    }
}
