package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.design.*
import kotlinx.serialization.json.JsonObject

/**
 * The user moved the bot to another login (iOS `UserSwitchRow`): a divider saying its next message carries the
 * conversation, and once it went, one that unfolds to what was carried.
 */
@Composable internal fun UserSwitchRow(change: JsonObject, state: AppState) {
    val carried = change.text("carried")?.takeIf { it.isNotEmpty() }
    val label = when (change.text("state")) {
        "pending" -> "Context handoff pending"
        "handedOff" -> "Context handed off"
        "switched" -> "Switched"
        else -> null
    }
    var expanded by remember { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Rule()
            val line = if (carried != null) Modifier.plainButton { expanded = !expanded } else Modifier
            Row(line, verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Symbol("arrow.left.arrow.right", size = 12.dp, tint = Palette.secondary)
                if (label != null) Text(tr(label), style = Type.caption, color = Palette.secondary, maxLines = 1)
                Side(change.text("from"), change.text("fromName"), state)
                Symbol("arrow.right", size = 10.dp, tint = Palette.secondary)
                Side(change.text("to"), change.text("toName"), state)
            }
            Rule()
        }
        AnimatedVisibility(expanded && carried != null, enter = fadeIn(), exit = fadeOut()) { CarriedText(carried.orEmpty()) }
    }
}

@Composable private fun Side(backend: String?, name: String?, state: AppState) {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        AgentIcon(backend?.let { agentRegistry(state, it) }, 12.dp)
        Text(name.orEmpty(), style = Type.caption, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

@Composable private fun RowScope.Rule() {
    Box(Modifier.weight(1f).height(1.dp).background(Palette.border))
}
