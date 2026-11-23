package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import com.codync.android.core.NodeKind
import com.codync.android.core.stepSymbol
import com.kyant.shapes.Capsule
import com.kyant.shapes.RoundedRectangle
import androidx.compose.foundation.rememberScrollState
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.codync.android.core.WorkflowRunDetail
import com.codync.android.core.WorkflowStep
import com.codync.android.core.stepStatusLabel
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch

@Composable internal fun WorkflowRunsView(state: AppState, store: AppStore, id: String, back: () -> Unit, open: (String) -> Unit) {
    var runs by remember(id) { mutableStateOf<List<com.codync.android.core.WorkflowRunSummary>>(emptyList()) }
    var hasMore by remember(id) { mutableStateOf(false) }
    var error by remember(id) { mutableStateOf<String?>(null) }
    var cursor by remember(id) { mutableStateOf<com.codync.android.core.WorkflowRunSummary?>(null) }
    var stamp by remember(id) { mutableStateOf(0) }
    LaunchedEffect(id, state.workflowTick) {
        cursor = null
        stamp++
    }
    LaunchedEffect(id, stamp, cursor) {
        val before = cursor
        try {
            val page = store.workflowRuns(id, 20, before)
            ensureActive()
            runs = if (before == null) page.runs else (runs + page.runs).distinctBy { it.id }
            hasMore = page.hasMore
            error = null
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message }
    }
    WorkflowScreen("Latest results", back) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            error?.let { Text(tr(it), style = Type.caption, color = Palette.danger) }
            for (run in runs) {
                Column(Modifier.fillMaxWidth().plainButton { open(run.id) }.padding(vertical = 8.dp)) {
                    Text(tr("Run {number}").replace("{number}", run.number.toString()), style = Type.body.semibold, color = Palette.text)
                    Text(workflowRunLine(run.status, run.startedAt, run.endedAt), style = Type.footnote, color = Palette.secondary)
                }
            }
            if (hasMore) LinkButton("Show older runs") { runs.lastOrNull()?.let { cursor = it } }
        }
    }
}

@Composable internal fun WorkflowRunView(state: AppState, store: AppStore, workflowId: String, runId: String, back: () -> Unit) {
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.WorkflowRun))
    var run by remember(runId) { mutableStateOf<WorkflowRunDetail?>(null) }
    var error by remember(runId) { mutableStateOf<String?>(null) }
    var openOutput by remember(runId) { mutableStateOf(setOf<String>()) }
    var openInput by remember(runId) { mutableStateOf(setOf<String>()) }
    var busy by remember(runId) { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(runId, state.workflowTick) {
        try {
            val loaded = store.fetchWorkflowRun(runId)
            ensureActive()
            run = loaded
            if (openOutput.isEmpty()) openOutput = loaded.steps.map { it.id }.toSet()
            error = null
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message }
    }
    val shown = run
    WorkflowScreen(tr("Run {number}").replace("{number}", (shown?.number ?: 0).toString()), back) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            error?.let { Text(tr(it), style = Type.callout, color = Palette.danger) }
            if (shown == null) Text(tr("Loading…"), style = Type.body, color = Palette.secondary)
            else {
                RunOverview(shown)
                shown.error?.let { Text(tr(it), style = Type.callout, color = Palette.danger) }
                if (shown.status in setOf("running", "approval", "stopping")) {
                    RunStop(enabled = !busy && shown.status != "stopping") {
                        scope.launch {
                            busy = true
                            try { store.stopWorkflowRun(shown.id) }
                            catch (cancelled: CancellationException) { throw cancelled }
                            catch (failure: Exception) { error = failure.message }
                            finally { busy = false }
                        }
                    }
                }
                val names = state::workflowBotName
                for (step in shown.steps) {
                    val node = shown.graph.nodes.firstOrNull { it.id == step.nodeId }
                    val title = node?.let { stepHeading(it, names) } ?: tr(step.title.ifBlank { step.kind })
                    RunStep(step, title, step.id in openOutput, step.id in openInput, busy, onToggle = {
                        openOutput = if (step.id in openOutput) openOutput - step.id else openOutput + step.id
                    }, onToggleInput = {
                        openInput = if (step.id in openInput) openInput - step.id else openInput + step.id
                    }, onRetry = {
                        scope.launch {
                            busy = true
                            try { store.retryWorkflowStep(step.id); error = null }
                            catch (cancelled: CancellationException) { throw cancelled }
                            catch (failure: Exception) { error = failure.message }
                            finally { busy = false }
                        }
                    }, onStop = {
                        scope.launch {
                            busy = true
                            try { store.stopWorkflowStep(step.id) }
                            catch (cancelled: CancellationException) { throw cancelled }
                            catch (failure: Exception) { error = failure.message }
                            finally { busy = false }
                        }
                    }, onDecide = { approved ->
                        scope.launch {
                            busy = true
                            try { store.decideWorkflowStep(step.id, approved) }
                            catch (cancelled: CancellationException) { throw cancelled }
                            catch (failure: Exception) { error = failure.message }
                            finally { busy = false }
                        }
                    })
                }
            }
        }
    }
}

@Composable private fun RunOverview(run: WorkflowRunDetail) {
    val finished = run.steps.count { it.status in setOf("done", "failed", "stopped", "skipped") }
    Column(Modifier.fillMaxWidth().clip(RoundedRectangle(16.dp)).background(Palette.surface).padding(14.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(workflowRunLine(run.status, run.startedAt, run.endedAt), Modifier.weight(1f), style = Type.subheadline, color = Palette.secondary)
            RunMark(run.status)
        }
        if (run.steps.isNotEmpty()) RunBar(finished.toFloat() / run.steps.size, run.status)
    }
}

@Composable private fun RunStep(step: WorkflowStep, title: String, expanded: Boolean, inputOpen: Boolean, busy: Boolean, onToggle: () -> Unit,
    onToggleInput: () -> Unit, onRetry: () -> Unit, onStop: () -> Unit, onDecide: (Boolean) -> Unit) {
    val live = step.status == "running" || step.status == "stopping"
    val ink = runInk(step.status)
    Column(Modifier.fillMaxWidth().clip(RoundedRectangle(16.dp)).background(if (live) Palette.bubbleUser else Palette.surface).padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(10.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Box(Modifier.size(32.dp).clip(RoundedCornerShape(9.dp)).background(Palette.background), contentAlignment = Alignment.Center) {
                Symbol(stepSymbol(NodeKind.fromWire(step.kind)), size = 16.dp, tint = Palette.text)
            }
            Text(title, Modifier.weight(1f), style = Type.body.medium, color = Palette.text, maxLines = 2, overflow = TextOverflow.Ellipsis)
            RunPill(step.status, ink)
        }
        step.error?.let { Text(tr(it), style = Type.caption, color = Palette.danger) }
        if (step.status == "approval") Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            PrimaryButton("Approve", enabled = !busy) { onDecide(true) }
            SecondaryButton("Deny", enabled = !busy) { onDecide(false) }
        }
        if (step.status == "failed") SecondaryButton("Retry", enabled = !busy, action = onRetry)
        if (step.status == "running") SecondaryButton("Stop", enabled = !busy, action = onStop)
        if (step.input.isNotBlank()) {
            Disclosure(tr(if (inputOpen) "Hide input" else "Show input"), inputOpen, onToggleInput)
            if (inputOpen) Quote(step.input)
        }
        val output = step.output
        if (!output.isNullOrBlank()) {
            Disclosure(outputCue(step, expanded), expanded, onToggle)
            if (expanded) Quote(output)
        }
    }
}

@Composable private fun RunMark(status: String) {
    val name = when (status) {
        "done" -> "checkmark.circle.fill"
        "failed" -> "xmark.circle.fill"
        "approval" -> "exclamationmark.circle.fill"
        "running", "stopping" -> "circle"
        else -> "circle"
    }
    Symbol(name, size = 18.dp, tint = runInk(status))
}

@Composable private fun RunPill(status: String, ink: Color) {
    val mark = when (status) {
        "done" -> "checkmark.circle.fill"
        "failed" -> "xmark.circle.fill"
        "approval" -> "exclamationmark.circle.fill"
        else -> null
    }
    Row(Modifier.clip(Capsule()).background(ink.copy(alpha = 0.14f)).padding(horizontal = 8.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        if (mark != null) Symbol(mark, size = 12.dp, tint = ink)
        Text(tr(stepStatusLabel(status)), style = Type.footnote.medium, color = ink, maxLines = 1)
    }
}

@Composable private fun RunBar(fraction: Float, status: String) {
    Box(Modifier.fillMaxWidth().height(4.dp).clip(Capsule()).background(Palette.accentDim)) {
        Box(Modifier.fillMaxWidth(fraction.coerceIn(0f, 1f)).height(4.dp).background(runInk(status)))
    }
}

@Composable private fun RunStop(enabled: Boolean, action: () -> Unit) {
    val dark = LocalDark.current
    Text(tr("Stop"), Modifier.clip(Capsule())
        .background(if (dark) Palette.danger.copy(alpha = 0.18f) else Palette.danger)
        .plainButton(enabled, action = action)
        .padding(horizontal = 22.dp, vertical = 11.dp),
        style = Type.body.medium, color = if (dark) Palette.danger else Color.White)
}

@Composable private fun Disclosure(title: String, open: Boolean, action: () -> Unit) {
    Row(Modifier.fillMaxWidth().plainButton(action = action), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(title, Modifier.weight(1f), style = Type.callout, color = Palette.secondary, maxLines = 2, overflow = TextOverflow.Ellipsis)
        Symbol("chevron.right", modifier = Modifier.rotate(if (open) 90f else 0f), size = 12.dp, tint = Palette.tertiary)
    }
}

@Composable private fun Quote(text: String) {
    Text(text, Modifier.fillMaxWidth().clip(RoundedRectangle(12.dp)).background(Palette.background).padding(12.dp), style = Type.callout, color = Palette.text)
}

@Composable private fun runInk(status: String): Color = when (status) {
    "done" -> Palette.added
    "failed" -> Palette.danger
    "approval" -> Palette.attention
    "running", "stopping" -> Palette.accent
    else -> Palette.secondary
}

@Composable private fun outputCue(step: WorkflowStep, open: Boolean): String {
    val verb = tr(if (open) "Hide output" else "Show output")
    val started = step.startedAt
    val ended = step.endedAt
    if (open || started == null || ended == null) return verb
    return "$verb · ${workflowStamp(ended)} · ${workflowDuration(started, ended)}"
}
