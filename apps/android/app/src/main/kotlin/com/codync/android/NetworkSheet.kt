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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.NetworkComputer
import com.codync.android.core.NetworkInfo
import com.codync.android.core.NetworkRule
import com.codync.android.core.NetworkView
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
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader({
            Symbol("codync.network", size = 24.dp, tint = Palette.text)
            Spacer(Modifier.width(8.dp))
            Text(tr("Network"), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Spacer(Modifier.width(8.dp))
            // The network is a beta feature.
            Text(tr("Beta"), style = Type.caption.semibold, color = Palette.secondary,
                modifier = Modifier.clip(Capsule()).background(Palette.bubbleAgent).padding(horizontal = 7.dp, vertical = 2.dp))
        })
        CardForm {
            // One section per network; who may message the bots here only where it has rules.
            for (network in view.networks) {
                CardSection(network.name) {
                    for (computer in network.computers) row { NetworkComputerRow(computer) }
                }
                if (network.rules.isNotEmpty()) CardSection("Who may message bots here", RULES_FOOTER) {
                    for (rule in network.rules) row { RuleRow(network, rule) }
                }
            }
            // The note that changes happen at a computer closes the networks, after the last one's rules.
            if (view.networks.isNotEmpty()) CardSection(footer = "Changes are made on a computer: in Codync's settings there, or with codync-host network.") {}
            if (view.assistantsAvailable) CardSection(footer = "ChatGPT or Claude in your Codync account can message this computer's bots when it's on. Codync's cloud can read these calls.") {
                row { ValueRow("Cloud assistants", if (view.assistants) "On" else "Off") }
            }
        }
    }
}

private const val RULES_FOOTER = "The first rule that matches decides. Without rules, every computer in the network may message every bot here. Answers to this computer's own messages always get in."

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
