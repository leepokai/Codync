package com.codync.android

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.ComputerSelection
import com.codync.android.core.HostRoute
import com.codync.android.core.LinkState
import com.codync.android.design.*

// The roster's computers (iOS `ComputerSection`, `ComputerFilterHeader`): a heading per computer once several show,
// and the header's filter. What's hidden or folded is kept on this device only (`RosterComputers`).

/**
 * A computer's heading above its bots: badge, name and connection. Tap folds its bots away; touch and hold,
 * then drag, moves the computer (on this device); [extensions] opens its own Extensions while it's reachable.
 */
@Composable internal fun ComputerSection(state: AppState, empty: Boolean, folded: Boolean, stale: Boolean, toggle: () -> Unit,
    move: (Int) -> Unit, extensions: (() -> Unit)?, modifier: Modifier = Modifier, drag: ContextDrag? = null) {
    val translate = translator()
    val turn by animateFloatAsState(if (folded) -90f else 0f, Motion.layout(), label = "Chevron")
    val link = state.connection
    val status = if (stale) computerStatusText(state, true) else statusText(state)
    val offline = (link is LinkState.ComputerOffline || link is LinkState.Failed) && !state.isUpdating
    Column(modifier.fillMaxWidth().padding(top = 22.dp, bottom = 2.dp)) {
        Row(Modifier.fillMaxWidth().contextActions(onClick = toggle, reorder = drag) { emptyList() }
            .clearAndSetSemantics {
                heading()
                contentDescription = "${state.hostName}, ${translate(status)}"
                stateDescription = translate(if (folded) "Collapsed" else "Expanded")
                customActions = listOfNotNull(
                    CustomAccessibilityAction(translate(if (folded) "Expand" else "Collapse")) { toggle(); true },
                    CustomAccessibilityAction(translate("Move up")) { move(-1); true },
                    CustomAccessibilityAction(translate("Move down")) { move(1); true },
                    extensions?.let { open -> CustomAccessibilityAction(translate("Extensions")) { open(); true } },
                )
            }.padding(vertical = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            ComputerBadge(state.hello?.text("device"), 20.dp, state.computer.color)
            // The name and the connection share the row; the connection sits at the trailing edge, before the heading's buttons.
            Text(state.hostName, Modifier.weight(1f), style = Type.subheadline.semibold, color = Palette.secondary,
                maxLines = 1, overflow = TextOverflow.Ellipsis)
            Box(Modifier.weight(1f).padding(start = 8.dp), contentAlignment = Alignment.CenterEnd) {
                Text(tr(status), style = Type.subheadline, color = if (offline) Palette.warning else Palette.tertiary, maxLines = 1,
                    overflow = TextOverflow.Ellipsis)
            }
            if (link is LinkState.Ready) Symbol(if (link.route == HostRoute.Direct) "wifi" else "cloud", size = 15.dp, tint = Palette.tertiary)
            Box(Modifier.size(40.dp), contentAlignment = Alignment.Center) {
                Symbol("chevron.down", Modifier.rotate(turn), size = 14.dp, tint = Palette.tertiary)
            }
            // After the chevron, as on the iPhone and the desktop.
            if (extensions != null) Box(Modifier.size(44.dp, 40.dp).semantics { contentDescription = translate("Extensions"); this.role = Role.Button }
                .plainButton(action = extensions), contentAlignment = Alignment.Center) {
                Symbol("square.grid.2x2", size = 16.dp, tint = Palette.secondary)
            }
        }
        AnimatedVisibility(empty && !folded, enter = fadeIn(Motion.fade()), exit = fadeOut(Motion.fade())) {
            Text(tr("No bots yet"), Modifier.padding(bottom = 4.dp), style = Type.footnote, color = Palette.tertiary)
        }
    }
}

/** The roster's only connection surface: a quiet summary, with the computers to show, reconnect and Computers on demand. */
@Composable internal fun ComputerFilterHeader(states: List<AppState>, hidden: String, setHidden: (String) -> Unit, reconnect: () -> Unit,
    manage: () -> Unit) {
    val selection = ComputerSelection(states.map { it.computer.id }, hidden)
    val shown = states.filter { it.computer.id in selection.shown }
    val translate = translator()
    val summary = connectionSummary(shown).let { it.filled(translate(it.template)) }
    DropdownMenu({
        listOf(
            MenuItem("All computers", icon = "desktopcomputer", selected = selection.shown.size == states.size) { setHidden("") },
            *states.map { state ->
                MenuItem(state.hostName, icon = computerSymbol(state.hello?.text("device")), selected = state.computer.id in selection.shown,
                    verbatim = true, detail = translate(connectionLabel(state))) { setHidden(selection.toggling(state.computer.id)) }
            }.toTypedArray(),
            *(if (states.size > 1) arrayOf(MenuItem("Show only", children = states.map { state ->
                MenuItem(state.hostName, icon = computerSymbol(state.hello?.text("device")), verbatim = true) { setHidden(selection.only(state.computer.id)) }
            })) else emptyArray()),
            MenuItem("Reconnect", icon = "arrow.clockwise", divider = true, enabled = shown.isNotEmpty(), action = reconnect),
            MenuItem("Manage computers", icon = "desktopcomputer", action = manage),
        )
    }, label = "Computers, $summary. Filter conversations") {
        Row(Modifier.padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(summary, style = Type.system(12f).medium, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Symbol("chevron.down", size = 15.dp, tint = Palette.secondary)
        }
    }
}
