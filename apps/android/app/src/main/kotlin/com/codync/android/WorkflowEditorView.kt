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
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.codync.android.core.EditMode
import com.codync.android.core.Graph
import com.codync.android.core.NodeKind
import com.codync.android.core.WorkflowDetail
import com.codync.android.core.WorkflowNode
import com.codync.android.design.*
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.Channel
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
    }
    val current = graph
    val mode = if (detail == null || current == null) EditMode.Fields else detail!!.copy(graph = current).phoneMode()
    WorkflowTitle(detail?.title.orEmpty(), back) {
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
                onAppend = { step -> appendLoose(current, step)?.let(::apply) })
        }
    }
    val editing = current?.nodes?.firstOrNull { it.id == sheet }
    val shownGraph = current
    CodyncSheet(editing != null && mode != EditMode.ReadOnly, { sheet = null }) {
        if (editing != null && shownGraph != null) WorkflowStepSheet(store, state, id, editing, detail?.webhooks.orEmpty()) { updated ->
            apply(shownGraph.copy(nodes = shownGraph.nodes.map { node -> if (node.id == updated.id) updated else node }))
        }
    }
}

@Composable private fun EditorSteps(graph: Graph, mode: EditMode, names: (String) -> String?, onOpen: (String) -> Unit, onMove: (String, Boolean) -> Unit,
    onDuplicate: (String) -> Unit, onDelete: (String) -> Unit, onAdd: (String, NodeKind) -> Unit, onAppend: (String) -> Unit) {
    val outline = if (mode == EditMode.Outline) outlineOf(graph) else null
    if (outline == null) {
        for (id in readingOrder(graph)) {
            val node = graph.nodes.firstOrNull { it.id == id } ?: continue
            Column(Modifier.fillMaxWidth().plainButton(enabled = mode != EditMode.ReadOnly) { onOpen(id) }.padding(vertical = 8.dp)) {
                StepLine(node, names)
            }
        }
        return
    }
    val unfed = unfedPrompts(outline, graph.nodes.associate { it.id to it.kind })
    val trigger = graph.nodes.firstOrNull { it.id == outline.trigger }
    if (trigger != null) OutlineRow(trigger, names, unfed, outline, onOpen, onMove, onDuplicate, onDelete, onAdd)
    ColumnSection(null, outline.main, graph, names, unfed, outline, onOpen, onMove, onDuplicate, onDelete, onAdd)
    val condition = outline.condition?.let { id -> graph.nodes.firstOrNull { it.id == id } }
    if (condition != null) OutlineRow(condition, names, unfed, outline, onOpen, onMove, onDuplicate, onDelete, onAdd)
    if (outline.condition != null) {
        ColumnSection("Yes", outline.yes, graph, names, unfed, outline, onOpen, onMove, onDuplicate, onDelete, onAdd)
        ColumnSection("No", outline.no, graph, names, unfed, outline, onOpen, onMove, onDuplicate, onDelete, onAdd)
        ColumnSection("After", outline.after, graph, names, unfed, outline, onOpen, onMove, onDuplicate, onDelete, onAdd)
    }
    if (outline.loose.isNotEmpty()) ColumnSection("Not connected", outline.loose, graph, names, unfed, outline, onOpen, onMove, onDuplicate, onDelete, onAdd, onAppend)
}

@Composable private fun ColumnSection(title: String?, ids: List<String>, graph: Graph, names: (String) -> String?, unfed: Set<String>, outline: Outline,
    onOpen: (String) -> Unit, onMove: (String, Boolean) -> Unit, onDuplicate: (String) -> Unit, onDelete: (String) -> Unit, onAdd: (String, NodeKind) -> Unit,
    onAppend: ((String) -> Unit)? = null) {
    if (ids.isEmpty() && title == null) return
    if (title != null) Text(tr(title), style = Type.footnote.semibold, color = Palette.secondary)
    for (id in ids) {
        val node = graph.nodes.firstOrNull { it.id == id } ?: continue
        OutlineRow(node, names, unfed, outline, onOpen, onMove, onDuplicate, onDelete, onAdd, onAppend)
    }
}

@Composable private fun OutlineRow(node: WorkflowNode, names: (String) -> String?, unfed: Set<String>, outline: Outline, onOpen: (String) -> Unit,
    onMove: (String, Boolean) -> Unit, onDuplicate: (String) -> Unit, onDelete: (String) -> Unit, onAdd: (String, NodeKind) -> Unit, onAppend: ((String) -> Unit)? = null) {
    val column = outline.columnOf(node.id)
    val items = column?.let { outline.items(it) }.orEmpty()
    val index = items.indexOf(node.id)
    Column(Modifier.fillMaxWidth().contextActions(onClick = { onOpen(node.id) }, items = {
        buildList {
            if (index > 0) add(MenuItem("Move up", icon = "chevron.up") { onMove(node.id, false) })
            if (index >= 0 && index < items.lastIndex) add(MenuItem("Move down", icon = "chevron.down") { onMove(node.id, true) })
            if (node.kind != NodeKind.Trigger && node.kind != NodeKind.Condition) add(MenuItem("Duplicate", icon = "plus") { onDuplicate(node.id) })
            add(MenuItem("Add after", icon = "plus", children = addMenu(node.id, outline, onAdd)))
            if (onAppend != null) add(MenuItem("Add to workflow", icon = "arrow.right") { onAppend(node.id) })
            if (node.kind != NodeKind.Trigger) add(MenuItem("Delete", icon = "trash", destructive = true) { onDelete(node.id) })
        }
    }).padding(vertical = 8.dp)) {
        StepLine(node, names)
        if (node.id in unfed) Text(tr("This prompt doesn't feed a step"), style = Type.caption, color = Palette.danger)
    }
}

private fun addMenu(after: String, outline: Outline, onAdd: (String, NodeKind) -> Unit): List<MenuItem> {
    val kinds = buildList {
        add(NodeKind.Agent to "Assistant")
        if (outline.condition == null && (after == outline.trigger || after in outline.main)) add(NodeKind.Condition to "Condition")
        add(NodeKind.Approval to "Approval")
        add(NodeKind.Prompt to "Prompt")
        add(NodeKind.Output to "Output")
    }
    return kinds.map { (kind, label) -> MenuItem(label) { onAdd(after, kind) } }
}

private fun fresh(kind: NodeKind) = WorkflowNode(UUID.randomUUID().toString(), kind, fields = when (kind) {
    NodeKind.Condition -> buildJsonObject { put("op", "contains"); put("value", "") }
    NodeKind.Approval -> buildJsonObject { put("question", "") }
    NodeKind.Prompt -> buildJsonObject { put("text", "") }
    NodeKind.Output -> buildJsonObject { put("target", "notify") }
    else -> JsonObject(emptyMap())
})

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
