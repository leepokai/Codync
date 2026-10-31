package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
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
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.EditMode
import com.codync.android.core.Graph
import com.codync.android.core.field
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
    val scope = rememberCoroutineScope()
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
    LaunchedEffect(id, shown?.running, state.workflowTick) {
        if (shown?.running == null) { openRun = null; return@LaunchedEffect }
        try { openRun = store.workflowRuns(id, 5).runs.firstOrNull { it.status in setOf("running", "approval", "stopping") } }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { }
    }
    WorkflowTitle(shown?.title ?: "", back, trailing = {
        IconButton("Rename", "pencil", enabled = shown != null) { renaming = true }
    }) {
        if (shown == null) {
            BoxFill(error)
        } else Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(22.dp)) {
            state.workflowMessage?.let { Text(tr(it), style = Type.callout, color = Palette.danger) }
            error?.let { Text(tr(it), style = Type.callout, color = Palette.danger) }
            SwitchRow("Pause", "A paused workflow doesn't start from its schedule or webhook. A run that's already going finishes.",
                isOn = !shown.enabled, enabled = !busy, onChange = { paused -> store.setWorkflowEnabled(id, !paused) })
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
            StepSection(shown.graph, names)
            TriggerSection(shown.graph, names)
            LatestRun(shown, openRun, showRuns)
            LinkButton("Versions", action = { versionsOn = true })
            LinkButton("Delete workflow", color = Palette.danger) { confirmDelete = true }
            Text(tr("Version {revision} · Saved {time}").replace("{revision}", shown.revision.toString()).replace("{time}", workflowStamp(shown.updatedAt)),
                style = Type.caption, color = Palette.tertiary)
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
}

@Composable private fun BoxFill(error: String?) {
    Column(Modifier.fillMaxSize().padding(24.dp), verticalArrangement = Arrangement.Center) {
        Text(tr(error ?: "Loading…"), style = Type.body, color = if (error == null) Palette.secondary else Palette.danger)
    }
}

@Composable private fun StepSection(graph: Graph, names: (String) -> String?) {
    CardSection("Steps") {
        for (id in readingOrder(graph)) {
            val node = graph.nodes.firstOrNull { it.id == id } ?: continue
            row { StepLine(node, names) }
        }
    }
}

@Composable private fun TriggerSection(graph: Graph, names: (String) -> String?) {
    val triggers = graph.nodes.filter { it.kind == NodeKind.Trigger }
    if (triggers.isEmpty()) return
    CardSection("Trigger") {
        for (node in triggers) row { StepLine(node, names) }
    }
}

@Composable internal fun StepLine(node: WorkflowNode, names: (String) -> String?) {
    val schedule = node.schedule?.field("expression")
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
        Symbol(stepSymbol(node.kind), size = 20.dp, tint = Palette.text)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(tr(node.stepTitle(names)), style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            val summary = node.stepSummary(names, schedule)
            if (summary.isNotBlank()) Text(tr(summary), style = Type.footnote, color = Palette.secondary, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    }
}

@Composable private fun LatestRun(detail: WorkflowDetail, openRun: WorkflowRunSummary?, showRuns: () -> Unit) {
    val last = detail.lastRun
    if (last == null && openRun == null) return
    CardSection("Latest results") {
        row {
            Column(Modifier.fillMaxWidth().plainButton(action = showRuns), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                val open = openRun
                val ended = last?.endedAt
                val text = when {
                    open != null -> workflowRunLine(open.status, open.startedAt, open.endedAt)
                    last != null && ended != null -> tr("{status} · {time}").replace("{status}", tr(runStatusLabel(last.status))).replace("{time}", workflowStamp(ended))
                    else -> tr(runStatusLabel(last?.status ?: detail.running ?: "done"))
                }
                Text(text, style = Type.body, color = Palette.text)
            }
        }
    }
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
