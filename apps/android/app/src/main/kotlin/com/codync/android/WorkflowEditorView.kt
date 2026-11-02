package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.core.EditMode
import com.codync.android.core.Graph
import com.codync.android.core.NodeKind
import com.codync.android.core.WorkflowDetail
import com.codync.android.core.WorkflowNode
import com.codync.android.core.field
import com.codync.android.core.stepSummary
import com.codync.android.core.stepSymbol
import com.codync.android.design.*
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.withContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

@Composable internal fun WorkflowEditorView(state: AppState, store: AppStore, id: String, back: () -> Unit) {
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.WorkflowEditor))
    var detail by remember(id) { mutableStateOf<WorkflowDetail?>(null) }
    var graph by remember(id) { mutableStateOf<Graph?>(null) }
    var revision by remember(id) { mutableLongStateOf(0L) }
    var error by remember(id) { mutableStateOf<String?>(null) }
    var conditionNote by remember(id) { mutableStateOf(false) }
    var sheet by remember(id) { mutableStateOf<String?>(null) }
    val kicks = remember(id) { Channel<Unit>(Channel.CONFLATED) }
    val scope = rememberCoroutineScope()
    fun apply(next: Graph) {
        graph = next
        store.holdWorkflow(id, revision, next)
        scope.launch { kicks.trySend(Unit) }
    }
    LaunchedEffect(id, state.workflowTick) {
        try {
            val fetched = store.fetchWorkflow(id)
            val held = store.heldWorkflow?.takeIf { it.id == id && store.isWorkflowDirty(id) }
            if (held != null) detail = fetched.copy(graph = held.graph, revision = held.revision)
            else {
                detail = fetched
                graph = fetched.graph
                revision = fetched.revision
            }
            if (graph == null) {
                graph = held?.graph ?: fetched.graph
                revision = held?.revision ?: fetched.revision
            }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message }
    }
    LaunchedEffect(id) {
        try {
        for (kick in kicks) {
            delay(400)
            val saving = graph ?: continue
            val host = detail ?: continue
            if (host.copy(graph = saving).phoneMode() == EditMode.ReadOnly) continue
            val base = revision
            try {
                val result = store.saveWorkflowGraph(id, base, saving)
                if (result.conflict) {
                    store.releaseWorkflow(id)
                    result.workflow?.let {
                        detail = it
                        graph = it.graph
                        revision = it.revision
                    }
                    sheet = null
                    conditionNote = false
                    while (kicks.tryReceive().getOrNull() != null) Unit
                    store.noteWorkflow("This workflow was edited somewhere else")
                } else {
                    val nextRevision = result.workflow?.revision ?: result.revision
                    revision = nextRevision
                    val latest = graph
                    if (latest != null && latest != saving) {
                        store.holdWorkflow(id, nextRevision, latest)
                        kicks.trySend(Unit)
                    } else {
                        store.releaseWorkflow(id)
                        result.workflow?.let { detail = it }
                    }
                    error = null
                }
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message ?: "Couldn't save this workflow." }
        }
        } finally {
            withContext(NonCancellable) { runCatching { store.flushHeldWorkflow() } }
        }
    }
    val current = graph
    val mode = if (detail == null || current == null) EditMode.Fields else detail!!.copy(graph = current).phoneMode()
    WorkflowTitle(detail?.title.orEmpty(), back, id) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            when (mode) {
                EditMode.Fields -> Text(tr("This workflow was arranged on a computer. You can edit each step's settings."), style = Type.callout, color = Palette.secondary)
                EditMode.Script -> Text(tr("This workflow is a script. You can change its trigger. The steps appear when it runs."), style = Type.callout, color = Palette.secondary)
                EditMode.ReadOnly -> Text(tr("Update Codync on this computer to edit this workflow."), style = Type.callout, color = Palette.secondary)
                EditMode.Outline -> Unit
            }
            error?.let { Text(tr(it), style = Type.callout, color = Palette.danger) }
            if (conditionNote) Text(tr("This workflow already has a condition"), style = Type.callout, color = Palette.danger)
            if (current != null) EditorSteps(current, mode, state::workflowBotName, onOpen = { if (mode != EditMode.ReadOnly) sheet = it },
                onMove = { step, down -> outlineOf(current)?.moved(step, down)?.let { apply(graphFromOutline(current, it)) } },
                onDuplicate = { step -> duplicate(current, step)?.let(::apply) },
                onDelete = { step -> removeStep(current, step)?.let(::apply) },
                onAdd = { after, kind ->
                    val added = addStep(current, after, kind)
                    if (added == null) conditionNote = true else { conditionNote = false; apply(added) }
                },
                onPlace = { column, kind -> placeStep(current, column, kind)?.let { conditionNote = false; apply(it) } },
                onAppend = { step -> appendLoose(current, step)?.let(::apply) })
        }
    }
    val editing = current?.nodes?.firstOrNull { it.id == sheet }
    val shownGraph = current
    CodyncSheet(editing != null && mode != EditMode.ReadOnly, { sheet = null }) {
        if (editing != null && shownGraph != null) {
            val (selection, options) = sourceChoices(shownGraph, editing.id, state::workflowBotName)
            WorkflowStepSheet(store, state, id, editing, detail?.webhooks.orEmpty(), selection, options, onSource = { token ->
                shownGraph.withSource(editing.id, token)?.let(::apply)
            }) { updated ->
                apply(shownGraph.copy(nodes = shownGraph.nodes.map { node -> if (node.id == updated.id) updated else node }))
            }
        }
    }
}

@Composable private fun EditorSteps(graph: Graph, mode: EditMode, names: (String) -> String?, onOpen: (String) -> Unit, onMove: (String, Boolean) -> Unit,
    onDuplicate: (String) -> Unit, onDelete: (String) -> Unit, onAdd: (String, NodeKind) -> Unit, onPlace: (OutlineColumn, NodeKind) -> Unit,
    onAppend: (String) -> Unit) {
    val numbers = stepNumbers(graph)
    val outline = if (mode == EditMode.Outline) outlineOf(graph) else null
    var adding by remember { mutableStateOf<OutlineColumn?>(null) }
    if (outline == null) {
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            for (id in readingOrder(graph)) {
                val node = graph.nodes.firstOrNull { it.id == id } ?: continue
                EditorCard(node, names, numbers[node.id], graph.parentsOf(id).mapNotNull { numbers[it] }, emptySet(), null, mode != EditMode.ReadOnly, onOpen, onMove, onDuplicate, onDelete, onAdd, null)
            }
        }
        return
    }
    val unfed = unfedPrompts(outline, graph.nodes.associate { it.id to it.kind })
    Column(verticalArrangement = Arrangement.spacedBy(22.dp)) {
        val trigger = graph.nodes.firstOrNull { it.id == outline.trigger }
        if (trigger != null) EditorColumn("Triggers", listOf(trigger.id), graph, numbers, names, unfed, outline, false, onOpen, onMove, onDuplicate, onDelete, onAdd, null) {}
        EditorColumn("Steps", outline.main, graph, numbers, names, unfed, outline, true, onOpen, onMove, onDuplicate, onDelete, onAdd, null) { adding = OutlineColumn.Main }
        val conditionId = outline.condition
        if (conditionId != null) {
            EditorColumn("Condition", listOf(conditionId), graph, numbers, names, unfed, outline, false, onOpen, onMove, onDuplicate, onDelete, onAdd, null) {}
            EditorColumn("Yes", outline.yes, graph, numbers, names, unfed, outline, true, onOpen, onMove, onDuplicate, onDelete, onAdd, null) { adding = OutlineColumn.Yes }
            EditorColumn("No", outline.no, graph, numbers, names, unfed, outline, true, onOpen, onMove, onDuplicate, onDelete, onAdd, null) { adding = OutlineColumn.No }
            EditorColumn("Then", outline.after, graph, numbers, names, unfed, outline, true, onOpen, onMove, onDuplicate, onDelete, onAdd, null) { adding = OutlineColumn.After }
        }
        if (outline.loose.isNotEmpty()) EditorColumn("Not connected", outline.loose, graph, numbers, names, unfed, outline, false, onOpen, onMove, onDuplicate, onDelete, onAdd, onAppend) {}
    }
    val column = adding
    WorkflowKindSheet(column != null, outline.condition != null, column == OutlineColumn.Main, { adding = null }) { kind ->
        val target = column ?: return@WorkflowKindSheet
        if (kind == NodeKind.Condition) onAdd(outline.main.lastOrNull() ?: outline.trigger, kind)
        else onPlace(target, kind)
        adding = null
    }
}

@Composable private fun EditorColumn(title: String, ids: List<String>, graph: Graph, numbers: Map<String, Int>, names: (String) -> String?, unfed: Set<String>, outline: Outline,
    allowAdd: Boolean, onOpen: (String) -> Unit, onMove: (String, Boolean) -> Unit, onDuplicate: (String) -> Unit, onDelete: (String) -> Unit,
    onAdd: (String, NodeKind) -> Unit, onAppend: ((String) -> Unit)?, ask: () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(tr(title), style = Type.caption, color = Palette.secondary)
        Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
            for (id in ids) {
                val node = graph.nodes.firstOrNull { it.id == id } ?: continue
                EditorCard(node, names, numbers[node.id], graph.parentsOf(node.id).mapNotNull { numbers[it] }, unfed, outline, true, onOpen, onMove, onDuplicate, onDelete, onAdd, onAppend)
            }
            if (allowAdd) WorkflowAddStepButton(ask)
        }
    }
}

@Composable private fun EditorCard(node: WorkflowNode, names: (String) -> String?, number: Int?, parents: List<Int>, unfed: Set<String>, outline: Outline?,
    tappable: Boolean, onOpen: (String) -> Unit, onMove: (String, Boolean) -> Unit, onDuplicate: (String) -> Unit, onDelete: (String) -> Unit,
    onAdd: (String, NodeKind) -> Unit, onAppend: ((String) -> Unit)?) {
    val items = outline?.columnOf(node.id)?.let { outline.items(it) }.orEmpty()
    val index = items.indexOf(node.id)
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        val open = Modifier.then(if (outline == null) Modifier.plainButton(enabled = tappable) { onOpen(node.id) }
            else Modifier.contextActions(onClick = { if (tappable) onOpen(node.id) }, items = {
                buildList {
                    if (index > 0) add(MenuItem("Move up", icon = "chevron.up") { onMove(node.id, false) })
                    if (index >= 0 && index < items.lastIndex) add(MenuItem("Move down", icon = "chevron.down") { onMove(node.id, true) })
                    if (node.kind != NodeKind.Trigger && node.kind != NodeKind.Condition) add(MenuItem("Duplicate", icon = "plus") { onDuplicate(node.id) })
                    if (outline != null) add(MenuItem("Add after", icon = "plus", children = addMenu(node.id, outline, onAdd)))
                    if (onAppend != null) add(MenuItem("Add to workflow", icon = "arrow.right") { onAppend(node.id) })
                    if (node.kind != NodeKind.Trigger) add(MenuItem("Delete", icon = "trash", destructive = true) { onDelete(node.id) })
                }
            }))
        WorkflowChoiceCard(stepSymbol(node.kind), stepHeading(node, names), editorDetail(node, names), translateTitle = false, number = number,
            parents = parents, caption = if (node.id in unfed) "This prompt doesn't feed a step" else null, modifier = Modifier.weight(1f).then(open))
        if (outline != null && node.kind != NodeKind.Trigger) IconButton("Delete", "trash") { onDelete(node.id) }
    }
}

@Composable private fun editorDetail(node: WorkflowNode, names: (String) -> String?): String {
    val summary = node.stepSummary(names, node.schedule?.field("expression"))
    if (summary.isNotBlank() && tr(summary) != stepHeading(node, names)) return summary
    if (node.kind != NodeKind.Trigger) return ""
    return when (node.fields.field("trigger")) {
        "schedule" -> "Runs at the time you set."
        "webhook" -> "Runs when its address receives a request."
        else -> "Runs when you start it."
    }
}

private fun addMenu(after: String, outline: Outline, onAdd: (String, NodeKind) -> Unit): List<MenuItem> {
    val kinds = buildList {
        add(NodeKind.Agent to "Agent")
        if (outline.condition == null && (after == outline.trigger || after in outline.main)) add(NodeKind.Condition to "Condition")
        add(NodeKind.Approval to "Approval")
        add(NodeKind.Prompt to "Prompt")
        add(NodeKind.Output to "Output")
    }
    return kinds.map { (kind, label) -> MenuItem(label) { onAdd(after, kind) } }
}

internal fun fresh(kind: NodeKind) = WorkflowNode(UUID.randomUUID().toString(), kind, fields = when (kind) {
    NodeKind.Condition -> buildJsonObject { put("op", "contains"); put("value", "") }
    NodeKind.Approval -> buildJsonObject { put("question", "") }
    NodeKind.Prompt -> buildJsonObject { put("text", "") }
    NodeKind.Output -> buildJsonObject { put("target", "notify") }
    else -> JsonObject(emptyMap())
})

private fun placeStep(graph: Graph, column: OutlineColumn, kind: NodeKind): Graph? {
    if (kind == NodeKind.Condition) return null
    val outline = outlineOf(graph) ?: return null
    val node = fresh(kind)
    return graphFromOutline(graph.copy(nodes = graph.nodes + node), outline.inserted(column, null, node.id))
}

private fun addStep(graph: Graph, after: String, kind: NodeKind): Graph? {
    val outline = outlineOf(graph) ?: return null
    if (kind == NodeKind.Condition) {
        if (outline.condition != null) return null
        val node = fresh(kind)
        val next = if (after == outline.trigger) outline.copy(condition = node.id, after = outline.main + outline.after, main = emptyList())
        else {
            val index = outline.main.indexOf(after)
            if (index < 0) return null
            outline.copy(main = outline.main.take(index + 1), condition = node.id, after = outline.main.drop(index + 1) + outline.after)
        }
        return graphFromOutline(graph.copy(nodes = graph.nodes + node), next)
    }
    val node = fresh(kind)
    val placed = when (after) {
        outline.trigger -> outline.copy(main = listOf(node.id) + outline.main)
        outline.condition -> outline.copy(after = listOf(node.id) + outline.after)
        else -> {
            val column = outline.columnOf(after) ?: return null
            outline.inserted(column, after, node.id)
        }
    }
    return graphFromOutline(graph.copy(nodes = graph.nodes + node), placed)
}

private fun duplicate(graph: Graph, id: String): Graph? {
    val outline = outlineOf(graph) ?: return null
    val node = graph.nodes.firstOrNull { it.id == id } ?: return null
    if (node.kind == NodeKind.Trigger || node.kind == NodeKind.Condition) return null
    val copy = node.copy(id = UUID.randomUUID().toString())
    val column = outline.columnOf(id) ?: return null
    return graphFromOutline(graph.copy(nodes = graph.nodes + copy), outline.inserted(column, id, copy.id))
}

private fun removeStep(graph: Graph, id: String): Graph? {
    val outline = outlineOf(graph) ?: return null
    if (id == outline.trigger) return null
    return graphFromOutline(graph.copy(nodes = graph.nodes.filter { it.id != id }), outline.without(id))
}

private fun appendLoose(graph: Graph, id: String): Graph? {
    val outline = outlineOf(graph) ?: return null
    if (id !in outline.loose) return null
    return graphFromOutline(graph, outline.copy(main = outline.main + id, loose = outline.loose - id))
}
