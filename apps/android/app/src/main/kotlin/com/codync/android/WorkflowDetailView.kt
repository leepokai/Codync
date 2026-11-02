package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.EditMode
import com.codync.android.core.Graph
import com.codync.android.core.field
import com.codync.android.core.stepWho
import com.codync.android.core.NodeKind
import com.codync.android.core.WorkflowDetail
import com.codync.android.core.WorkflowNode
import com.codync.android.core.WorkflowRunSummary
import com.codync.android.core.WorkflowVersion
import com.codync.android.core.runStatusLabel
import com.codync.android.core.stepSummary
import com.codync.android.core.stepSymbol
import com.codync.android.core.stepTitle
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

@Composable internal fun WorkflowDetailView(state: AppState, store: AppStore, id: String, back: () -> Unit, edit: () -> Unit, showRuns: () -> Unit) {
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Workflow))
    var detail by remember(id) { mutableStateOf<WorkflowDetail?>(null) }
    var openRun by remember(id) { mutableStateOf<WorkflowRunSummary?>(null) }
    var error by remember(id) { mutableStateOf<String?>(null) }
    var busy by remember(id) { mutableStateOf(false) }
    var confirmDelete by remember(id) { mutableStateOf(false) }
    var renaming by remember(id) { mutableStateOf(false) }
    var versionsOn by remember(id) { mutableStateOf(false) }
    var recent by remember(id) { mutableStateOf<List<WorkflowRunSummary>>(emptyList()) }
    var moreRuns by remember(id) { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    var lines by rememberSaveable { mutableStateOf(StepDisplay.lines(context)) }
    var pickingDisplay by rememberSaveable { mutableStateOf(false) }
    LaunchedEffect(id, state.workflowTick) {
        try {
            val fetched = store.fetchWorkflow(id)
            val held = store.heldWorkflow?.takeIf { it.id == id && store.isWorkflowDirty(id) }
            detail = if (held == null) fetched else fetched.copy(graph = held.graph, revision = held.revision)
            error = null
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message }
    }
    val shown = detail
    LaunchedEffect(id, shown?.lastRun?.number, state.workflowTick) {
        try {
            val page = store.workflowRuns(id, 3)
            recent = page.runs
            moreRuns = page.hasMore
        }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { }
    }
    LaunchedEffect(id, shown?.running, state.workflowTick) {
        if (shown?.running == null) { openRun = null; return@LaunchedEffect }
        try { openRun = store.workflowRuns(id, 5).runs.firstOrNull { it.status in setOf("running", "approval", "stopping") } }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { }
    }
    WorkflowTitle(shown?.title ?: "", back, id, trailing = {
        ModeButton(lines) { pickingDisplay = true }
        IconButton("Rename", "pencil", enabled = shown != null) { renaming = true }
    }) {
        if (shown == null) {
            BoxFill(error)
        } else Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(22.dp)) {
            state.workflowMessage?.let { Text(tr(it), style = Type.callout, color = Palette.danger) }
            error?.let { Text(tr(it), style = Type.callout, color = Palette.danger) }
            SwitchRow("Pause", "A paused workflow doesn't start from its schedule or webhook. A run that's already going finishes.",
                isOn = !shown.enabled, enabled = !busy, onChange = { paused -> store.setWorkflowEnabled(id, !paused) })
            val count = stepCount(shown.graph)
            if (count > 0) Text(tr("{count} steps").replace("{count}", count.toString()), style = Type.footnote, color = Palette.secondary)
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                val running = openRun
                if (running != null) PrimaryButton("Stop", Modifier.weight(1f), !busy && running.status != "stopping") {
                    scope.launch {
                        busy = true
                        try { store.stopWorkflowRun(running.id); error = null }
                        catch (cancelled: CancellationException) { throw cancelled }
                        catch (failure: Exception) { error = failure.message }
                        finally { busy = false }
                    }
                } else PrimaryButton("Run now", Modifier.weight(1f), !busy) {
                    scope.launch {
                        busy = true
                        try {
                            val run = store.startWorkflowRun(id)
                            error = null
                            store.showWorkflow(id, run.id)
                        } catch (cancelled: CancellationException) { throw cancelled }
                        catch (failure: Exception) { error = failure.message ?: "Couldn't start this workflow." }
                        finally { busy = false }
                    }
                }
                SecondaryButton("Edit", Modifier.weight(1f), enabled = shown.phoneMode() != EditMode.ReadOnly, action = edit)
            }
            val names = state::workflowBotName
            StepSection(shown.graph, names, lines)
            TriggerSection(shown.graph, names)
            RunHistory(recent, moreRuns, { store.showWorkflow(id, it) }, showRuns)
            CardSection {
                row {
                    Text(tr("Delete workflow"), Modifier.fillMaxWidth().plainButton { confirmDelete = true },
                        style = Type.body, color = Palette.danger)
                }
            }
            Text(tr("Version {revision} · Saved {time}").replace("{revision}", shown.revision.toString()).replace("{time}", workflowStamp(shown.updatedAt)),
                Modifier.plainButton { versionsOn = true }, style = Type.caption, color = Palette.tertiary)
        }
    }
    CodyncDialog("Delete workflow?", confirmDelete, { confirmDelete = false }, "This can't be undone.") {
        listOf(DialogAction("Delete workflow", destructive = true) { store.deleteWorkflow(id) })
    }
    var draftTitle by remember(shown?.title) { mutableStateOf(shown?.title.orEmpty()) }
    CodyncDialog("Rename", renaming, { renaming = false }, field = {
        FieldBox(draftTitle, { draftTitle = it }, "Workflow", imeAction = ImeAction.Done, actions = KeyboardActions(onDone = {
            if (draftTitle.isNotBlank()) store.renameWorkflow(id, draftTitle.trim())
            renaming = false
        }))
    }) { listOf(DialogAction("Save") { if (draftTitle.isNotBlank()) store.renameWorkflow(id, draftTitle.trim()) }) }
    WorkflowVersionsSheet(versionsOn, { versionsOn = false }, store, id)
    WorkflowDisplaySheet(pickingDisplay, lines, { pickingDisplay = false }) { chosen ->
        lines = chosen
        StepDisplay.set(context, chosen)
    }
}

@Composable private fun BoxFill(error: String?) {
    Column(Modifier.fillMaxSize().padding(24.dp), verticalArrangement = Arrangement.Center) {
        Text(tr(error ?: "Loading…"), style = Type.body, color = if (error == null) Palette.secondary else Palette.danger)
    }
}

@Composable private fun StepSection(graph: Graph, names: (String) -> String?, lines: Boolean) {
    val footer = if (outlineOf(graph) == null) "The steps show from left to right, as on the canvas." else null
    val numbers = stepNumbers(graph)
    if (lines) {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            SectionHeading("Steps")
            WorkflowStepFlow(graph, names)
            if (footer != null) Text(tr(footer), Modifier.padding(horizontal = 2.dp), style = Type.footnote, color = Palette.secondary)
        }
    } else CardSection("Steps", footer) {
        for (id in readingOrder(graph)) {
            val node = graph.nodes.firstOrNull { it.id == id } ?: continue
            if (node.kind == NodeKind.Trigger || node.kind.isAnnotation) continue
            row { StepLine(node, names, numbers[node.id], graph.parentsOf(node.id).mapNotNull { numbers[it] }) }
        }
    }
}

private fun stepCount(graph: Graph): Int = readingOrder(graph).count { id ->
    val kind = graph.nodes.firstOrNull { it.id == id }?.kind ?: return@count false
    kind.isStep && kind != NodeKind.Trigger
}

@Composable private fun TriggerSection(graph: Graph, names: (String) -> String?) {
    val triggers = graph.nodes.filter { it.kind == NodeKind.Trigger }
    if (triggers.isEmpty()) return
    val numbers = stepNumbers(graph)
    CardSection("Trigger") {
        for (node in triggers) row { StepLine(node, names, parents = graph.parentsOf(node.id).mapNotNull { numbers[it] }) }
    }
}

@Composable internal fun stepHeading(node: WorkflowNode, names: (String) -> String?): String {
    val kind = tr(node.stepTitle())
    val who = node.stepWho(names)
    return if (who.isNullOrBlank()) kind else "$kind · $who"
}

@Composable internal fun StepLine(node: WorkflowNode, names: (String) -> String?, number: Int? = null, parents: List<Int> = emptyList()) {
    val schedule = node.schedule?.field("expression")
    val heading = stepHeading(node, names)
    val summary = tr(node.stepSummary(names, schedule))
    val showsDetail = summary.isNotBlank() && summary != heading
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
        if (number != null) Text(number.toString(), style = Type.body.semibold, color = Palette.text)
        Box(Modifier.size(32.dp).background(Palette.surface, RoundedCornerShape(9.dp)), contentAlignment = Alignment.Center) {
            Symbol(stepSymbol(node.kind), size = 16.dp, tint = Palette.text)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(heading, style = if (showsDetail) Type.footnote else Type.body,
                color = if (showsDetail) Palette.secondary else Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (showsDetail) Text(summary, style = Type.body, color = Palette.text, maxLines = 2, overflow = TextOverflow.Ellipsis)
            StepSources(parents)
        }
    }
}

@Composable private fun RunHistory(runs: List<WorkflowRunSummary>, more: Boolean, openRun: (String) -> Unit, showOlder: () -> Unit) {
    if (runs.isEmpty()) return
    val shown = runs.take(3)
    CardSection("Run history") {
        for (run in shown) row(onClick = { openRun(run.id) }) {
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    Text(tr("Run {number}").replace("{number}", run.number.toString()), style = Type.body, color = Palette.text, maxLines = 1)
                    Text(runSubtitle(run), style = Type.footnote, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
                Symbol("chevron.right", size = 14.dp, tint = Palette.tertiary)
            }
        }
        if (more) row(onClick = showOlder) {
            Text(tr("Show older runs"), Modifier.fillMaxWidth(), style = Type.body, color = Palette.text)
        }
    }
}

@Composable private fun runSubtitle(run: WorkflowRunSummary): String {
    val parts = mutableListOf(tr(runStatusLabel(run.status)))
    if (run.startedAt > 0) parts += java.text.DateFormat.getDateInstance(java.text.DateFormat.MEDIUM).format(java.util.Date(run.startedAt))
    run.endedAt?.let { parts += workflowDuration(run.startedAt, it) }
    return parts.joinToString(" · ")
}

@Composable private fun WorkflowVersionsSheet(open: Boolean, dismiss: () -> Unit, store: AppStore, id: String) {
    CodyncSheet(open, dismiss) {
        var versions by remember { mutableStateOf<List<WorkflowVersion>>(emptyList()) }
        var picked by remember { mutableStateOf<WorkflowVersion?>(null) }
        var preview by remember { mutableStateOf<Graph?>(null) }
        var error by remember { mutableStateOf<String?>(null) }
        var busy by remember { mutableStateOf(false) }
        val scope = rememberCoroutineScope()
        LaunchedEffect(open) {
            if (!open) return@LaunchedEffect
            try { versions = store.workflowVersions(id); error = null }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message }
        }
        Column(Modifier.fillMaxSize().background(Palette.background)) {
            ModalHeader("Versions")
            Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                error?.let { Text(tr(it), style = Type.caption, color = Palette.danger) }
                for (version in versions) {
                    val on = picked?.id == version.id
                    Column(Modifier.fillMaxWidth().plainButton {
                        picked = version
                        scope.launch {
                            try { preview = store.fetchWorkflowVersion(id, version.id); error = null }
                            catch (cancelled: CancellationException) { throw cancelled }
                            catch (failure: Exception) { error = failure.message }
                        }
                    }.padding(vertical = 6.dp)) {
                        Text(versionTitle(version), style = Type.body.semibold, color = Palette.text)
                        if (version.kind == "restore" || version.kind == "run") Text(workflowStamp(version.createdAt), style = Type.footnote, color = Palette.secondary)
                    }
                    if (on) Text(tr("Read only"), style = Type.caption, color = Palette.secondary)
                }
                preview?.let { Text(tr("{count} steps").replace("{count}", it.nodes.count { node -> node.kind.isStep }.toString()), style = Type.caption, color = Palette.tertiary) }
            }
            val chosen = picked
            if (chosen != null) PrimaryButton("Make current", Modifier.padding(20.dp).fillMaxWidth(), !busy) {
                scope.launch {
                    busy = true
                    try {
                        store.restoreWorkflowVersion(id, chosen.id)
                        dismiss()
                    } catch (cancelled: CancellationException) { throw cancelled }
                    catch (failure: Exception) { error = failure.message }
                    finally { busy = false }
                }
            }
        }
    }
}

@Composable private fun versionTitle(version: WorkflowVersion): String = when (version.kind) {
    "restore" -> tr("Before restore")
    "run" -> tr("When a run started")
    else -> workflowStamp(version.createdAt)
}
