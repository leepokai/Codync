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
import com.codync.android.core.stepTitle
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

@Composable internal fun WorkflowRunsView(state: AppState, store: AppStore, id: String, back: () -> Unit, open: (String) -> Unit) {
    var limit by remember(id) { mutableStateOf(20) }
    var runs by remember(id) { mutableStateOf<List<com.codync.android.core.WorkflowRunSummary>>(emptyList()) }
    var hasMore by remember(id) { mutableStateOf(false) }
    var error by remember(id) { mutableStateOf<String?>(null) }
    LaunchedEffect(id, limit, state.workflowTick) {
        try {
            val page = store.workflowRuns(id, limit)
            runs = page.runs
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
            if (hasMore) LinkButton("Show older runs") { limit += 20 }
        }
    }
}

@Composable internal fun WorkflowRunView(state: AppState, store: AppStore, workflowId: String, runId: String, back: () -> Unit) {
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.WorkflowRun))
    var run by remember(runId) { mutableStateOf<WorkflowRunDetail?>(null) }
    var error by remember(runId) { mutableStateOf<String?>(null) }
    var openOutput by remember(runId) { mutableStateOf(setOf<String>()) }
    var busy by remember(runId) { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    LaunchedEffect(runId, state.workflowTick) {
        try { run = store.fetchWorkflowRun(runId); error = null }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (failure: Exception) { error = failure.message }
    }
    val shown = run
    WorkflowScreen(tr("Run {number}").replace("{number}", (shown?.number ?: 0).toString()), back) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(20.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            error?.let { Text(tr(it), style = Type.callout, color = Palette.danger) }
            if (shown == null) Text(tr("Loading…"), style = Type.body, color = Palette.secondary)
            else {
                Text(workflowRunLine(shown.status, shown.startedAt, shown.endedAt), style = Type.subheadline, color = Palette.secondary)
                shown.error?.let { Text(tr(it), style = Type.callout, color = Palette.danger) }
                if (shown.status in setOf("running", "approval")) {
                    PrimaryButton("Stop", enabled = !busy) {
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
                    val title = node?.stepTitle(names) ?: step.title.ifBlank { step.kind }
                    RunStep(step, title, step.id in openOutput, busy, onToggle = {
                        openOutput = if (step.id in openOutput) openOutput - step.id else openOutput + step.id
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

@Composable private fun RunStep(step: WorkflowStep, title: String, expanded: Boolean, busy: Boolean, onToggle: () -> Unit, onRetry: () -> Unit,
    onStop: () -> Unit, onDecide: (Boolean) -> Unit) {
    val status = stepStatusLabel(step.status)
    Column(Modifier.fillMaxWidth().cardGroup().padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(tr(title), Modifier.weight(1f), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(tr(status), style = Type.footnote.medium, color = when (step.status) {
                "failed" -> Palette.danger
                "approval" -> Palette.attention
                else -> Palette.secondary
            })
        }
        step.error?.let { Text(tr(it), style = Type.caption, color = Palette.danger) }
        if (step.status == "approval") Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            PrimaryButton("Approve", enabled = !busy) { onDecide(true) }
            SecondaryButton("Deny", enabled = !busy) { onDecide(false) }
        }
        if (step.status == "failed") SecondaryButton("Retry", enabled = !busy, action = onRetry)
        if (step.status == "running") SecondaryButton("Stop", enabled = !busy, action = onStop)
        val output = step.output
        if (!output.isNullOrBlank()) {
            LinkButton(if (expanded) "Hide output" else "Show output", action = onToggle)
            if (expanded) Text(output, style = Type.footnote.monospaced, color = Palette.text)
        }
    }
}
