package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.FlowCanvas
import com.codync.android.core.FlowLink
import com.codync.android.core.FlowRoutineCard
import com.codync.android.core.RoutineFlow
import com.codync.android.core.flowStatusLabel
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle

/** What the detail sheet is about: a routine or a bot (by node key), or a link (by id). */
internal sealed interface FlowSelection {
    data class OfNode(val key: String) : FlowSelection
    data class OfLink(val id: String) : FlowSelection
}

/** A step's status as a small chip: words and a symbol, never color alone. */
@Composable internal fun StatusChip(status: String?) {
    val symbol = when (status) {
        "running" -> "arrow.triangle.2.circlepath"; "succeeded" -> "checkmark.circle"; "failed" -> "xmark.circle"
        "skipped" -> "minus.circle"; "cancelled" -> "stop.circle"; else -> "circle.dashed"
    }
    val tint = when (status) { "succeeded" -> Palette.added; "failed" -> Palette.danger; "running" -> Palette.accent; else -> Palette.secondary }
    Row(Modifier.clip(RoundedRectangle(50.dp)).background(Palette.text.copy(alpha = 0.07f)).padding(horizontal = 8.dp, vertical = 2.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Symbol(symbol, size = 12.dp, tint = tint)
        Text(tr(flowStatusLabel(status)), style = Type.caption, color = tint, maxLines = 1)
    }
}

/** The sheet over the canvas or the list: what the selected routine, bot or link says, and the actions on it. */
@Composable internal fun FlowDetail(owner: Bot, canvas: FlowCanvas, bots: Map<String, Bot>, takers: List<Bot>, selection: FlowSelection,
    online: Boolean, busy: Boolean, store: AppStore, note: (String?) -> Unit, reselect: (FlowSelection) -> Unit, close: () -> Unit) {
    val unknownBot = tr("Unknown bot")
    val name = { id: String -> bots[id]?.name ?: unknownBot }
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(when (selection) {
            is FlowSelection.OfLink -> tr("Link")
            is FlowSelection.OfNode -> if (selection.key.startsWith("routine:")) tr("Routine") else tr("Bot")
        })
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            when (selection) {
                is FlowSelection.OfLink -> canvas.links.firstOrNull { it.id == selection.id }?.let { link ->
                    LinkPanel(owner, link, name, online, busy, store, close)
                } ?: Text(tr("This link is gone."), style = Type.footnote, color = Palette.secondary)
                is FlowSelection.OfNode -> {
                    val id = selection.key.substringAfter(':')
                    if (selection.key.startsWith("routine:")) canvas.routines.firstOrNull { it.routine.id == id }?.let { card ->
                        RoutinePanel(owner, canvas, card, name, online, busy, store)
                    } else BotPanel(owner, canvas, bots[id], id, takers, name, online, busy, store, note, reselect, close)
                }
            }
        }
    }
}

@Composable private fun LinkPanel(owner: Bot, link: FlowLink, name: (String) -> String, online: Boolean, busy: Boolean, store: AppStore, close: () -> Unit) {
    var text by remember(link.id) { mutableStateOf(link.instruction) }
    LaunchedEffect(link.instruction) { text = link.instruction }
    Text("${name(link.fromBotId)} → ${name(link.toBotId)}", style = Type.headline, color = Palette.text)
    Text(tr("What the next bot is asked to do with the reply. It can be empty."), style = Type.caption, color = Palette.secondary)
    Field("Instruction") { FieldBox(text, { text = it.take(8000) }, "e.g. Summarize it in three lines", 3, 8, fill = Palette.bubbleUser) }
    PrimaryButton(tr("Save"), Modifier.fillMaxWidth(), enabled = online && !busy && text.trim() != link.instruction) {
        store.updateFlowLink(owner.id, link.id, text.trim())
    }
    SecondaryButton(tr("Disconnect"), Modifier.fillMaxWidth(), enabled = online && !busy) { store.disconnectFlow(owner.id, link.id); close() }
}

@Composable private fun RoutinePanel(owner: Bot, canvas: FlowCanvas, card: FlowRoutineCard, name: (String) -> String, online: Boolean, busy: Boolean, store: AppStore) {
    val routine = card.routine
    val linked = !RoutineFlow.isUnconnected(canvas, routine.id)
    Text(routine.name, style = Type.headline, color = Palette.text)
    Text(routine.triggerDescriptions.joinToString(" · "), style = Type.footnote, color = Palette.secondary)
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(tr(if (routine.enabled) "Active" else "Paused"), Modifier.weight(1f), style = Type.footnote, color = Palette.text)
        CodyncSwitch(routine.enabled, { store.setFlowRoutineEnabled(owner.id, routine.id, it) },
            Modifier.width(34.dp).semantics { contentDescription = routine.name }, enabled = online && !busy) {}
    }
    routine.nextRunAt?.let { Text("${tr("Next run")}: ${java.text.DateFormat.getDateTimeInstance().format(java.util.Date(it))}", style = Type.caption, color = Palette.secondary) }
    Text(card.recentRuns.firstOrNull()?.let { "${tr("Last run")}: ${tr(it.label)}" } ?: tr("No runs yet"), style = Type.caption, color = Palette.secondary)
    if (!linked) Text(tr("Not connected"), style = Type.caption, color = Palette.secondary)
    PrimaryButton(tr("Test the flow"), Modifier.fillMaxWidth(), enabled = online && !busy && linked) { store.testFlow(owner.id, routine.id) }
    if (card.steps.isNotEmpty()) {
        Text(tr("Latest run"), style = Type.caption, color = Palette.secondary)
        for (step in card.steps) Column(Modifier.fillMaxWidth().clip(RoundedRectangle(8.dp)).background(Palette.text.copy(alpha = 0.05f)).padding(8.dp),
            verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(name(step.botId), Modifier.weight(1f), style = Type.footnote, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                StatusChip(step.status)
            }
            step.error?.let { Text(it, style = Type.caption, color = Palette.danger, maxLines = 3, overflow = TextOverflow.Ellipsis) }
            step.output?.let { Text(it, style = Type.caption, color = Palette.secondary, maxLines = 3, overflow = TextOverflow.Ellipsis) }
        }
    }
    if (card.recentRuns.isNotEmpty()) {
        Text(tr("Run history"), style = Type.caption, color = Palette.secondary)
        for (run in card.recentRuns) Row(Modifier.fillMaxWidth()) {
            Text(tr(run.label), Modifier.weight(1f), style = Type.caption, color = Palette.text)
            Text(java.text.DateFormat.getDateTimeInstance().format(java.util.Date(run.createdAt)), style = Type.caption, color = Palette.secondary)
        }
    }
}

@Composable private fun BotPanel(owner: Bot, canvas: FlowCanvas, bot: Bot?, id: String, takers: List<Bot>, name: (String) -> String,
    online: Boolean, busy: Boolean, store: AppStore, note: (String?) -> Unit, reselect: (FlowSelection) -> Unit, close: () -> Unit) {
    val step = RoutineFlow.latestStep(canvas, id)
    Text(name(id), style = Type.headline, color = Palette.text)
    Text(bot?.model ?: bot?.backend.orEmpty(), style = Type.footnote, color = Palette.secondary)
    StatusChip(step?.status)
    step?.error?.let { Text(it, style = Type.caption, color = Palette.danger, maxLines = 4, overflow = TextOverflow.Ellipsis) }
    // Bots this one may hand a reply to: each with the routines of this canvas that allow it.
    val options = takers.mapNotNull { target ->
        val routines = canvas.routines.filter { RoutineFlow.connectProblem(canvas, it.routine.id, id, target.id) == null }
        if (target.id == id || routines.isEmpty()) null else target to routines
    }
    fun connect(routine: String, to: String) {
        note(null)
        store.connectFlow(owner.id, routine, id, to) { link -> reselect(FlowSelection.OfLink(link.id)) }
    }
    DropdownMenu({ options.map { (target, routines) ->
        if (routines.size == 1) MenuItem(target.name) { connect(routines.single().routine.id, target.id) }
        else MenuItem(target.name, children = routines.map { card -> MenuItem(card.routine.name) { connect(card.routine.id, target.id) } })
    } }, enabled = online && !busy && options.isNotEmpty(), label = tr("Hand its reply on to…")) {
        Text(tr(if (options.isEmpty()) "No bot to hand on to" else "Hand its reply on to…"),
            Modifier.fillMaxWidth().clip(RoundedRectangle(12.dp)).background(Palette.bubbleUser).padding(14.dp), style = Type.footnote, color = Palette.text)
    }
    val removable = id in canvas.placedBotIds && canvas.links.none { it.fromBotId == id || it.toBotId == id }
    if (removable) SecondaryButton(tr("Remove from the canvas"), Modifier.fillMaxWidth(), enabled = online && !busy) {
        store.removeFlowNode(owner.id, RoutineFlow.botKey(id)); close()
    }
}
