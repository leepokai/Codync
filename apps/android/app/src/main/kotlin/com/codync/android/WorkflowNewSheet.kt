package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.Graph
import com.codync.android.core.NodeKind
import com.codync.android.core.WorkflowNode
import com.codync.android.core.field
import com.codync.android.core.stepSummary
import com.codync.android.core.stepSymbol
import com.codync.android.core.stepWho
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import java.time.ZoneId
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** Name, trigger and steps, then one save. The host still owns the saved graph. */
@Composable internal fun WorkflowNewSheet(presented: Boolean, onDismiss: () -> Unit, store: AppStore, state: AppState, created: (String) -> Unit) {
    var name by remember(presented) { mutableStateOf("") }
    var trigger by remember(presented) { mutableStateOf("manual") }
    var steps by remember(presented) { mutableStateOf(listOf<WorkflowNode>()) }
    var picking by remember(presented) { mutableStateOf(false) }
    var adding by remember(presented) { mutableStateOf(false) }
    var editingId by remember(presented) { mutableStateOf<String?>(null) }
    var saving by remember(presented) { mutableStateOf(false) }
    var error by remember(presented) { mutableStateOf<String?>(null) }
    val scope = rememberCoroutineScope()
    CodyncSheet(presented, onDismiss) {
        Column(Modifier.fillMaxWidth()) {
            ModalHeader("New workflow") {
                IconButton("Save", "checkmark", enabled = !saving) {
                    scope.launch {
                        saving = true
                        try {
                            val detail = store.createWorkflow(name.trim().ifBlank { "Workflow" }, composed(trigger, steps))
                            created(detail.id)
                            onDismiss()
                        } catch (cancelled: CancellationException) { throw cancelled }
                        catch (failure: Exception) { error = failure.message }
                        finally { saving = false }
                    }
                }
            }
            Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal = 20.dp).padding(bottom = 24.dp),
                verticalArrangement = Arrangement.spacedBy(22.dp)) {
                error?.let { Text(tr(it), style = Type.caption, color = Palette.danger) }
                Labeled("Name") { FieldBox(name, { name = it }, "Name", fill = Palette.bubbleUser) }
                Labeled("Triggers") {
                    ChoiceRow(stepSymbol(NodeKind.Trigger), triggerTitle(trigger), triggerDetail(trigger), chevron = true) { picking = true }
                }
                Labeled("Steps") {
                    Column(verticalArrangement = Arrangement.spacedBy(10.dp)) {
                        steps.forEach { node ->
                            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                ChoiceRow(stepSymbol(node.kind), stepTitle(node, state), rowDetail(node, state), chevron = true,
                                    translateTitle = false, modifier = Modifier.weight(1f)) { editingId = node.id }
                                IconButton("Delete", "trash") { steps = steps.filter { it.id != node.id } }
                            }
                        }
                        Row(Modifier.fillMaxWidth().clip(RoundedRectangle(16.dp)).background(Palette.surface)
                            .plainButton { adding = true }.padding(vertical = 14.dp),
                            horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
                            Symbol("plus", size = 16.dp, tint = Palette.text)
                            Text(tr("Add step"), Modifier.padding(start = 8.dp), style = Type.body.medium, color = Palette.text)
                        }
                    }
                }
            }
        }
    }
    WorkflowTriggerSheet(picking, trigger, { picking = false }) { trigger = it }
    WorkflowKindSheet(adding, steps.any { it.kind == NodeKind.Condition }, onDismiss = { adding = false }) { steps = steps + fresh(it) }
    val editing = steps.firstOrNull { it.id == editingId }
    CodyncSheet(editing != null, { editingId = null }) {
        editing?.let { node ->
            val (selection, options) = draftChoices(steps, node.id, trigger, state)
            WorkflowStepSheet(store, state, "", node, emptyList(), selection, options, onSource = { token ->
                steps = retargetDraft(steps, node.id, token)
            }) { updated ->
                steps = steps.map { if (it.id == updated.id) updated else it }
            }
        }
    }
}

@Composable private fun Labeled(title: String, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(tr(title), style = Type.caption, color = Palette.secondary)
        content()
    }
}

@Composable private fun ChoiceRow(symbol: String, title: String, detail: String, chevron: Boolean, translateTitle: Boolean = true,
    modifier: Modifier = Modifier, action: () -> Unit) {
    Row(modifier.fillMaxWidth().clip(RoundedRectangle(16.dp)).background(Palette.surface).plainButton(action = action).padding(14.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.size(36.dp).clip(RoundedCornerShape(10.dp)).background(Palette.background), contentAlignment = Alignment.Center) {
            Symbol(symbol, size = 16.dp, tint = Palette.text)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(if (translateTitle) tr(title) else title, style = Type.body.medium, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(tr(detail), style = Type.caption, color = Palette.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
        if (chevron) Symbol("chevron.right", size = 12.dp, tint = Palette.tertiary)
    }
}

@Composable private fun WorkflowTriggerSheet(presented: Boolean, selection: String, onDismiss: () -> Unit, choose: (String) -> Unit) {
    val rows = listOf(
        Triple("manual", "Manual", "Runs when you start it."),
        Triple("schedule", "Schedule", "Runs at the time you set."),
        Triple("webhook", "Webhook", "Runs when its address receives a request."),
    )
    CodyncSheet(presented, onDismiss, fitted = true) {
        Column(Modifier.fillMaxWidth()) {
            ModalHeader("Triggers")
            Text(tr("How it starts"), Modifier.padding(horizontal = 20.dp), style = Type.caption, color = Palette.secondary)
            Column(Modifier.padding(horizontal = 20.dp).padding(top = 8.dp, bottom = 20.dp).clip(RoundedRectangle(16.dp)).background(Palette.surface)) {
                for ((id, title, detail) in rows) {
                    Row(Modifier.fillMaxWidth().plainButton { choose(id); onDismiss() }.padding(horizontal = 16.dp, vertical = 14.dp),
                        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
                            Text(tr(title), style = Type.body.medium, color = Palette.text)
                            Text(tr(detail), style = Type.caption, color = Palette.secondary, maxLines = 2)
                        }
                        if (selection == id) Symbol("checkmark", size = 16.dp, tint = Palette.text)
                    }
                }
            }
        }
    }
}

@Composable internal fun WorkflowKindSheet(presented: Boolean, hasCondition: Boolean, allowCondition: Boolean = true, onDismiss: () -> Unit, add: (NodeKind) -> Unit) {
    val kinds = listOf(
        Triple(NodeKind.Agent, "Agent", "An agent finishes this step with its tools."),
        Triple(NodeKind.Condition, "Condition", "A yes or no that splits into two paths."),
        Triple(NodeKind.Approval, "Approval", "The run waits for you to decide."),
        Triple(NodeKind.Prompt, "Prompt", "Fixed text for the next step."),
        Triple(NodeKind.Output, "Output", "Sends the result to you."),
    )
    CodyncSheet(presented, onDismiss, fitted = true) {
        Column(Modifier.fillMaxWidth().padding(bottom = 20.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            ModalHeader("Add step")
            Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                for ((kind, title, detail) in kinds) {
                    if (kind == NodeKind.Condition && !allowCondition) Unit
                    else if (kind == NodeKind.Condition && hasCondition) {
                        Text(tr("This workflow already has a condition"), Modifier.fillMaxWidth().clip(RoundedRectangle(16.dp))
                            .background(Palette.surface).padding(14.dp), style = Type.callout, color = Palette.secondary)
                    } else {
                        ChoiceRow(stepSymbol(kind), title, detail, chevron = false) { add(kind); onDismiss() }
                    }
                }
            }
        }
    }
}

private fun triggerTitle(id: String) = when (id) {
    "schedule" -> "Schedule"
    "webhook" -> "Webhook"
    else -> "Manual"
}

private fun triggerDetail(id: String) = when (id) {
    "schedule" -> "Runs at the time you set."
    "webhook" -> "Runs when its address receives a request."
    else -> "Runs when you start it."
}

@Composable private fun stepTitle(node: WorkflowNode, state: AppState): String {
    val kind = tr(kindTitle(node.kind))
    val who = node.stepWho(state::workflowBotName) ?: return kind
    return "$kind · $who"
}

/** The explanation until this step has something of its own, then that text. */
private fun rowDetail(node: WorkflowNode, state: AppState): String {
    val written = node.stepSummary(state::workflowBotName)
    return when (node.kind) {
        NodeKind.Prompt -> if (node.fields.field("text").isNullOrBlank()) "Fixed text for the next step." else written
        NodeKind.Agent -> if (node.fields.field("task").isNullOrBlank() && node.fields.field("botId").isNullOrBlank())
            "An agent finishes this step with its tools." else written
        NodeKind.Approval -> if (node.fields.field("question").isNullOrBlank()) "The run waits for you to decide." else written
        NodeKind.Output -> if (node.fields.field("target") == "chat") written else "Sends the result to you."
        NodeKind.Condition -> {
            val value = node.fields.field("value").orEmpty().trim()
            val op = node.fields.field("op") ?: "contains"
            if (value.isEmpty() && op == "contains") "A yes or no that splits into two paths."
            else when (op) {
                "equals" -> if (value.isEmpty()) "Equals" else "Equals $value"
                "regex" -> if (value.isEmpty()) "Matches" else "Matches $value"
                "empty" -> "Empty"
                else -> if (value.isEmpty()) "Contains" else "Contains $value"
            }
        }
        else -> written
    }
}

@Composable private fun draftChoices(steps: List<WorkflowNode>, id: String, trigger: String, state: AppState): Pair<String, List<Pair<String, String>>> {
    val (outline, kinds) = draftOutline(steps)
    val numbers = steps.mapIndexed { index, node -> node.id to index + 1 }.toMap()
    val titles = steps.associate { it.id to stepTitle(it, state) }
    val label = when (trigger) {
        "schedule" -> "Schedule"
        "webhook" -> "Webhook"
        else -> "Manual"
    }
    return outlineChoices(outline, kinds, id, label, branches = false, number = { numbers[it] }, title = { titles[it].orEmpty() })
}

private fun draftOutline(steps: List<WorkflowNode>): Pair<Outline, Map<String, NodeKind>> {
    val main = mutableListOf<String>()
    var condition: String? = null
    val after = mutableListOf<String>()
    val kinds = mutableMapOf("*trigger" to NodeKind.Trigger)
    for (node in steps) {
        kinds[node.id] = node.kind
        if (node.kind == NodeKind.Condition && condition == null) condition = node.id
        else if (condition == null) main.add(node.id)
        else after.add(node.id)
    }
    return Outline(trigger = "*trigger", main = main, condition = condition, after = after) to kinds
}

private fun retargetDraft(steps: List<WorkflowNode>, id: String, token: String): List<WorkflowNode> {
    val (outline, kinds) = draftOutline(steps)
    val target = SourceTarget.from(token)
    if (outline.sourceTarget(id, kinds) == target) return steps
    val next = outline.retargeted(id, target) ?: return steps
    val order = next.main + listOfNotNull(next.condition) + next.yes + next.no + next.after
    val byId = steps.associateBy { it.id }
    return order.mapNotNull { byId[it] }
}

private fun kindTitle(kind: NodeKind) = when (kind) {
    NodeKind.Agent -> "Agent"
    NodeKind.Condition -> "Condition"
    NodeKind.Approval -> "Approval"
    NodeKind.Prompt -> "Prompt"
    NodeKind.Output -> "Output"
    else -> "Note"
}

/** A straight outline. One condition sends the steps after it down both paths. A schedule starts at 09:00 every day. */
private fun composed(trigger: String, steps: List<WorkflowNode>): Graph {
    val zone = ZoneId.systemDefault().id
    val triggerNode = WorkflowNode(UUID.randomUUID().toString(), NodeKind.Trigger, fields = buildJsonObject {
        put("trigger", trigger)
        if (trigger == "schedule") put("timezone", zone)
    }, schedule = if (trigger == "schedule") buildJsonObject {
        put("type", "cron")
        put("expression", "0 9 * * *")
        put("timeZone", zone)
    } else null)
    val nodes = mutableListOf(triggerNode)
    val main = mutableListOf<String>()
    var condition: String? = null
    val after = mutableListOf<String>()
    for (node in steps) {
        nodes.add(node)
        if (node.kind == NodeKind.Condition && condition == null) condition = node.id
        else if (condition == null) main.add(node.id)
        else after.add(node.id)
    }
    return graphFromOutline(Graph(nodes), Outline(triggerNode.id, main, condition, after = after))
}
