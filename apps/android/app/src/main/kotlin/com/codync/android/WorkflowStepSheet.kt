package com.codync.android

import android.content.ClipData
import android.content.ClipDescription
import android.os.Build
import android.os.PersistableBundle
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
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
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.NodeKind
import com.codync.android.core.RoutineScheduleDraft
import com.codync.android.core.WorkflowNode
import com.codync.android.core.WorkflowStep
import com.codync.android.core.WorkflowWebhook
import com.codync.android.core.field
import com.codync.android.core.routineRepeats
import com.codync.android.core.withField
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import java.time.ZoneId
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch

@Composable internal fun WorkflowStepSheet(store: AppStore, state: AppState, workflowId: String, node: WorkflowNode, webhooks: List<WorkflowWebhook>,
    sourceSelection: String = "", sourceOptions: List<Pair<String, String>> = emptyList(), onSource: (String) -> Unit = {},
    onChange: (WorkflowNode) -> Unit) {
    Column(Modifier.fillMaxSize().background(Palette.background)) {
        ModalHeader(title = {
            Text(stepHeading(node, state::workflowBotName), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        })
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            if (sourceOptions.isNotEmpty()) {
                SheetField("Source") { ChoicePicker(sourceSelection, sourceOptions, onSource, label = "Source", wide = true) }
            }
            when (node.kind) {
                NodeKind.Trigger -> TriggerFields(store, workflowId, node, webhooks, onChange)
                NodeKind.Agent -> AgentFields(state, node, onChange)
                NodeKind.Condition -> ConditionFields(node, onChange)
                NodeKind.Approval -> TextCard("Question") {
                    FieldBox(node.fields.field("question").orEmpty(), { onChange(node.copy(fields = node.fields.withField("question", it))) }, "Approve this step?", 2, 6, fill = Palette.bubbleUser)
                }
                NodeKind.Prompt -> TextCard("Prompt") {
                    FieldBox(node.fields.field("text").orEmpty(), { onChange(node.copy(fields = node.fields.withField("text", it))) }, "Prompt", 3, 8, fill = Palette.bubbleUser)
                }
                NodeKind.Output -> OutputFields(state, node, onChange)
                else -> Text(tr("Update Codync on this computer to edit this workflow."), style = Type.callout, color = Palette.secondary)
            }
            if (workflowId.isNotEmpty() && node.kind in setOf(NodeKind.Agent, NodeKind.Condition, NodeKind.Approval, NodeKind.Output)) {
                SoloRun(store, workflowId, node.id)
            }
        }
    }
}

@Composable private fun TriggerFields(store: AppStore, workflowId: String, node: WorkflowNode, webhooks: List<WorkflowWebhook>, onChange: (WorkflowNode) -> Unit) {
    val trigger = node.fields.field("trigger") ?: "manual"
    SheetField("When to run") {
        ChoicePicker(trigger, listOf("manual" to "Manual", "schedule" to "Schedule", "webhook" to "Webhook"), { next ->
            val cleared = if (next == "schedule") node.fields else node.fields.withField("timezone", null)
            onChange(node.copy(fields = cleared.withField("trigger", next), schedule = if (next == "schedule") node.schedule else null))
        }, label = "When to run", wide = true)
    }
    when (trigger) {
        "schedule" -> ScheduleFields(store, node, onChange)
        "webhook" -> WebhookFields(store, workflowId, node.id, webhooks.firstOrNull { it.nodeId == node.id })
        else -> Text(tr("Manual"), style = Type.caption, color = Palette.secondary)
    }
}

@Composable private fun ScheduleFields(store: AppStore, node: WorkflowNode, onChange: (WorkflowNode) -> Unit) {
    var draft by remember(node.id) {
        mutableStateOf(RoutineScheduleDraft(calendarStyle = "custom", expression = node.schedule?.field("expression").orEmpty(),
            zone = node.fields.field("timezone") ?: node.schedule?.field("timeZone") ?: ZoneId.systemDefault().id))
    }
    var touched by remember(node.id) { mutableStateOf(false) }
    var error by remember(node.id) { mutableStateOf<String?>(null) }
    var choosingZone by remember(node.id) { mutableStateOf(false) }
    fun edit(next: RoutineScheduleDraft) { touched = true; draft = next }
    LaunchedEffect(draft) {
        if (!touched) return@LaunchedEffect
        try {
            val preview = store.routineSchedule(draft)
            ensureActive()
            if (node.fields.field("trigger") != "schedule") return@LaunchedEffect
            val cron = preview.triggers.firstOrNull { it.field("type") == "cron" } ?: return@LaunchedEffect
            val zone = cron.field("timeZone").orEmpty()
            val expression = cron.field("expression").orEmpty()
            if (zone.isBlank() || expression.isBlank()) return@LaunchedEffect
            val next = node.copy(schedule = cron, fields = node.fields.withField("trigger", "schedule").withField("timezone", zone))
            if (next.schedule != node.schedule || next.fields.field("timezone") != node.fields.field("timezone")) onChange(next)
            error = null
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message ?: "Couldn't check this schedule." }
    }
    SheetField("Repeat") { ChoicePicker(draft.calendarStyle, routineRepeats, { edit(draft.repeating(it, draft.expression)) }, label = "Repeat", wide = true) }
    when (draft.calendarStyle) {
        "hourly" -> SheetField("At minute") { ChoicePicker(draft.minute, minutes, { edit(draft.copy(minute = it)) }, label = "At minute", wide = true) }
        "daily", "weekdays" -> TimeRow(draft) { edit(it) }
        "weekly" -> {
            SheetField("Weekday") { ChoicePicker(draft.weekday, weekdays, { edit(draft.copy(weekday = it)) }, label = "Weekday", wide = true) }
            TimeRow(draft) { edit(it) }
        }
        else -> TextCard("Cron") {
            FieldBox(draft.expression, { edit(draft.copy(expression = it)) }, "0 9 * * 1-5", style = Type.body.monospaced, fill = Palette.bubbleUser, capitalize = false)
        }
    }
    SheetField("Time zone") { ZoneButton(draft.zone) { choosingZone = true } }
    error?.let { Text(tr(it), style = Type.caption, color = Palette.danger) }
    CodyncSheet(choosingZone, { choosingZone = false }) {
        ZonePicker(draft.zone) { edit(draft.copy(zone = it)); choosingZone = false }
    }
}

@Composable private fun TimeRow(draft: RoutineScheduleDraft, change: (RoutineScheduleDraft) -> Unit) = SheetField("At") {
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        ChoicePicker(draft.hour, hours, { change(draft.copy(hour = it)) }, label = "Hour")
        Text(":", style = Type.body, color = Palette.secondary)
        ChoicePicker(draft.minute, minutes, { change(draft.copy(minute = it)) }, label = "Minute")
    }
}

@Composable private fun AgentFields(state: AppState, node: WorkflowNode, onChange: (WorkflowNode) -> Unit) {
    val selected = node.fields.field("botId").orEmpty()
    val options = listOf("" to "Choose a bot") + state.workflowAgents().map { it.id to it.name }
    SheetField("Agent") { ChoicePicker(selected, options, { onChange(node.copy(fields = node.fields.withField("botId", it.ifBlank { null }))) }, label = "Agent", wide = true) }
    state.workflowModel(selected)?.let { Text(it, style = Type.footnote, color = Palette.secondary) }
    TextCard("Task") {
        FieldBox(node.fields.field("task").orEmpty(), { onChange(node.copy(fields = node.fields.withField("task", it.ifBlank { null }))) }, "What this step should do", 3, 8, fill = Palette.bubbleUser)
    }
}

@Composable private fun ConditionFields(node: WorkflowNode, onChange: (WorkflowNode) -> Unit) {
    val op = node.fields.field("op") ?: "contains"
    SheetField("Matches") {
        ChoicePicker(op, listOf("contains" to "Contains", "equals" to "Equals", "regex" to "Regex", "empty" to "Empty"), {
            onChange(node.copy(fields = node.fields.withField("op", it)))
        }, label = "Matches", wide = true)
    }
    if (op != "empty") TextCard("Value") {
        FieldBox(node.fields.field("value").orEmpty(), { onChange(node.copy(fields = node.fields.withField("value", it))) }, "Value", fill = Palette.bubbleUser, capitalize = op != "regex")
    }
}

@Composable private fun OutputFields(state: AppState, node: WorkflowNode, onChange: (WorkflowNode) -> Unit) {
    val target = node.fields.field("target") ?: "notify"
    SheetField("Send to") {
        ChoicePicker(target, listOf("notify" to "Notification", "chat" to "Chat"), { onChange(node.copy(fields = node.fields.withField("target", it))) }, label = "Send to", wide = true)
    }
    if (target == "chat") {
        val selected = node.fields.field("botId").orEmpty()
        val options = listOf("" to "Choose a bot") + state.workflowAgents().map { it.id to it.name }
        SheetField("Agent") { ChoicePicker(selected, options, { onChange(node.copy(fields = node.fields.withField("botId", it.ifBlank { null }))) }, label = "Agent", wide = true) }
    }
}

@Composable private fun WebhookFields(store: AppStore, workflowId: String, nodeId: String, known: WorkflowWebhook?) {
    var webhook by remember(nodeId) { mutableStateOf(known) }
    var revealed by remember(nodeId) { mutableStateOf(known?.secret) }
    var show by remember(nodeId) { mutableStateOf(false) }
    var confirm by remember(nodeId) { mutableStateOf(false) }
    var error by remember(nodeId) { mutableStateOf<String?>(null) }
    var copied by remember(nodeId) { mutableStateOf<String?>(null) }
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    LaunchedEffect(nodeId) {
        try {
            val fresh = store.fetchWorkflow(workflowId, reveal = true).webhooks.firstOrNull { it.nodeId == nodeId }
            if (fresh != null) { webhook = fresh; revealed = fresh.secret }
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message }
    }
    val url = webhook?.url?.takeIf { it.isNotBlank() } ?: webhook?.localUrl
    if (url.isNullOrBlank()) Text(tr("Runs each time something is posted to its URL. Save to get the URL and key."), style = Type.caption, color = Palette.secondary)
    else {
        fun copy(field: String, value: String, sensitive: Boolean) = scope.launch {
            val clip = ClipData.newPlainText(field, value)
            if (sensitive && Build.VERSION.SDK_INT >= 33) clip.description.extras = PersistableBundle().apply { putBoolean(ClipDescription.EXTRA_IS_SENSITIVE, true) }
            clipboard.setClipEntry(ClipEntry(clip))
            copied = field
        }
        SecretRow("URL", url) { IconButton(if (copied == "url") "Copied" else "Copy", if (copied == "url") "checkmark" else "doc.on.doc") { copy("url", url, false) } }
        val secret = revealed
        if (secret != null) SecretRow("Key", if (show) secret else "•".repeat(16)) {
            IconButton(if (show) "Hide key" else "Show key", if (show) "eye.slash" else "eye") { show = !show }
            IconButton("Replace key", "arrow.triangle.2.circlepath") { confirm = true }
            IconButton(if (copied == "key") "Copied" else "Copy", if (copied == "key") "checkmark" else "doc.on.doc") { copy("key", secret, true) }
        }
        error?.let { Text(tr(it), style = Type.caption, color = Palette.danger) }
    }
    CodyncDialog("Replace the key?", confirm, { confirm = false }, "Senders using the current key stop working until they get the new one.") {
        listOf(DialogAction("Replace key", destructive = true) {
            scope.launch {
                try {
                    revealed = store.rotateWorkflowWebhook(workflowId, nodeId)
                    show = true
                    error = null
                } catch (cancelled: CancellationException) { throw cancelled }
                catch (failure: Exception) { error = failure.message }
            }
        })
    }
}

@Composable private fun SheetField(label: String, content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(tr(label), style = Type.caption, color = Palette.secondary)
        content()
    }
}

@Composable private fun TextCard(label: String, content: @Composable () -> Unit) {
    Column(Modifier.fillMaxWidth().clip(RoundedRectangle(16.dp)).background(Palette.surface).padding(14.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(tr(label), style = Type.caption, color = Palette.secondary)
        content()
    }
}

@Composable private fun SecretRow(label: String, value: String, buttons: @Composable () -> Unit) = SheetField(label) {
    Row(Modifier.fillMaxWidth().clip(RoundedRectangle(12.dp)).background(Palette.bubbleUser).padding(start = 12.dp, end = 6.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Box(Modifier.weight(1f)) { SelectionContainer { Text(value, style = Type.callout.monospaced, color = Palette.text, maxLines = 3, overflow = TextOverflow.Ellipsis) } }
        buttons()
    }
}

@Composable private fun SoloRun(store: AppStore, workflowId: String, nodeId: String) {
    var step by remember(nodeId) { mutableStateOf<WorkflowStep?>(null) }
    var error by remember(nodeId) { mutableStateOf<String?>(null) }
    var busy by remember(nodeId) { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    error?.let { Text(tr(it), style = Type.caption, color = Palette.danger) }
    val running = step?.status == "running"
    PrimaryButton(if (running) "Stop" else "Run", Modifier.fillMaxWidth(), enabled = !busy, action = {
        scope.launch {
            busy = true
            try {
                val current = step
                if (current?.status == "running") {
                    store.stopWorkflowStep(current.id)
                    step = current.copy(status = "stopped")
                } else step = store.runWorkflowStep(workflowId, nodeId)
                error = null
            } catch (cancelled: CancellationException) { throw cancelled }
            catch (failure: Exception) { error = failure.message }
            finally { busy = false }
        }
    })
}

private fun Int.two() = toString().padStart(2, '0')
private val hours = (0..23).map { it to it.two() }
private val minutes = (0..59).map { it to it.two() }
private val weekdays = listOf(1 to "Monday", 2 to "Tuesday", 3 to "Wednesday", 4 to "Thursday", 5 to "Friday", 6 to "Saturday", 0 to "Sunday")
