package com.codync.android

import android.content.Context
import android.view.accessibility.AccessibilityManager
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.FlowCanvas
import com.codync.android.core.LinkState
import com.codync.android.core.RoutineFlow
import com.codync.android.core.flowStatusLabel
import com.codync.android.design.*

private fun Context.screenReaderOn(): Boolean =
    (getSystemService(Context.ACCESSIBILITY_SERVICE) as? AccessibilityManager)?.isTouchExplorationEnabled == true

/**
 * A bot's routine flow: the canvas with its nodes and links, or the same flow as a list. The list is the
 * default with TalkBack on and on narrow screens, where a canvas is hard to use; both open the same detail sheet.
 */
@Composable fun RoutineFlowView(bot: Bot, state: AppState, store: AppStore) {
    val online = state.actualConnection is LinkState.Ready
    val canvas = state.flows[bot.id]
    val busy = "flow:${bot.id}" in state.busy
    val context = LocalContext.current
    val narrow = LocalConfiguration.current.screenWidthDp < 600
    var listMode by remember(bot.id) { mutableStateOf(context.screenReaderOn() || narrow) }
    var selection by remember(bot.id) { mutableStateOf<FlowSelection?>(null) }
    var note by remember(bot.id) { mutableStateOf<String?>(null) }
    val rev = state.flowRevs[bot.id] ?: 0
    // Loads when the screen opens, whenever the connection returns and when the host says the canvas changed.
    LaunchedEffect(bot.id, online, rev) { if (online) store.loadFlow(bot.id) }
    val byId = state.bots.associateBy { it.id }
    val takers = state.bots.filter { it.kind != "group" && !it.hidden && it.backend != "external" }

    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        if (canvas == null) {
            Text(tr(if (online) "Loading the flow…" else "Offline: connect to this computer to see the flow."),
                Modifier.outlinedCard(vertical = 14.dp), style = Type.footnote, color = Palette.secondary)
            return@Column
        }
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            SegmentedChoice(listMode, listOf(false to tr("Canvas"), true to tr("List")), { listMode = it }, Modifier.weight(1f))
            val onCanvas = RoutineFlow.botIds(canvas).toSet()
            val addable = takers.filter { it.id !in onCanvas }
            DropdownMenu({ addable.map { target -> MenuItem(target.name) { placeBot(store, bot.id, canvas, target.id) } } },
                enabled = online && !busy && addable.isNotEmpty(), label = tr("Add a bot to the canvas")) {
                Text(tr(if (addable.isEmpty()) "Every bot is on the canvas" else "Add a bot"), Modifier.padding(horizontal = 10.dp, vertical = 8.dp),
                    style = Type.footnote, color = Palette.text)
            }
        }
        if (!online) Text(tr("Offline: the flow is read-only until the computer is reachable."), style = Type.caption, color = Palette.attention)
        note?.let { Text(it, style = Type.caption, color = Palette.danger) }
        if (canvas.routines.isEmpty()) {
            Text(tr("No routines yet. Ask the bot for one, then connect it to other bots here."),
                Modifier.outlinedCard(vertical = 14.dp), style = Type.footnote, color = Palette.secondary)
        } else if (listMode) {
            FlowList(canvas, byId) { selection = it }
        } else {
            FlowCanvasView(canvas, byId, online && !busy, { selection = it }, { key, x, y -> store.moveFlowNode(bot.id, key, x, y) })
        }
    }

    val shown = selection
    CodyncSheet(shown != null, { selection = null }, setOf(SheetDetent.Medium, SheetDetent.Expanded)) {
        if (canvas != null && shown != null) FlowDetail(bot, canvas, byId, takers, shown, online, busy, store,
            { text -> note = text }, { selection = it }, { selection = null })
    }
}

private fun placeBot(store: AppStore, canvasBot: String, canvas: FlowCanvas, bot: String) {
    val nodes = RoutineFlow.layout(canvas)
    val x = (nodes.maxOfOrNull { it.x + RoutineFlow.NODE_WIDTH } ?: 0.0) + 90.0
    store.placeFlowBot(canvasBot, bot, x, nodes.minOfOrNull { it.y } ?: 0.0)
}

// MARK: the list (what TalkBack and a narrow screen walk)

@Composable private fun FlowList(canvas: FlowCanvas, bots: Map<String, Bot>, select: (FlowSelection) -> Unit) {
    val rows = RoutineFlow.rows(canvas)
    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
        for (card in canvas.routines) {
            val routine = card.routine
            Column(Modifier.outlinedCard(vertical = 4.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                Row(Modifier.fillMaxWidth().clickable { select(FlowSelection.OfNode(RoutineFlow.routineKey(routine.id))) }.padding(vertical = 10.dp),
                    verticalAlignment = Alignment.CenterVertically) {
                    Text(routine.name, Modifier.weight(1f), style = Type.system(13f), color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    if (RoutineFlow.isUnconnected(canvas, routine.id)) Text(tr("Not connected"), style = Type.caption, color = Palette.secondary)
                }
                for (row in rows.filter { it.routineId == routine.id }) {
                    val name = bots[row.botId]?.name ?: tr("Unknown bot")
                    val status = tr(flowStatusLabel(row.step?.status))
                    Row(Modifier.fillMaxWidth()
                        .clickable { select(row.link?.let { FlowSelection.OfLink(it.id) } ?: FlowSelection.OfNode(RoutineFlow.botKey(row.botId))) }
                        .semantics(mergeDescendants = true) { contentDescription = "$name, $status" }
                        .padding(start = (row.depth * 14).dp, top = 8.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Column(Modifier.weight(1f)) {
                            Text((if (row.depth > 0) "→ " else "") + name, style = Type.footnote, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            row.link?.instruction?.takeIf { it.isNotBlank() }?.let {
                                Text(it, style = Type.caption, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            }
                        }
                        StatusChip(row.step?.status)
                    }
                }
            }
        }
    }
}
