package com.codync.android

import android.content.Context
import java.text.DateFormat
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.asPaddingValues
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.LinkState
import com.codync.android.core.WorkflowSummary
import com.codync.android.core.statusLabel
import com.codync.android.core.workflowMatches
import com.codync.android.design.*
import com.kyant.shapes.Capsule
import java.util.Date
import kotlinx.coroutines.CancellationException

/** The Workflows tab, view only: list, one workflow, its runs, or one run, where an approval step is answered. */
@Composable internal fun WorkflowsView(app: AccountContext, account: () -> Unit, state: AppState, store: AppStore, computers: List<AppState>, bottomInset: Dp, selectComputer: (String) -> Unit) {
    val route = state.workflow ?: return
    var runsOn by rememberSaveable { mutableStateOf(false) }
    val back = remember { mutableStateOf<() -> Unit>({}) }
    val latest = rememberUpdatedState(route to runsOn)
    back.value = {
        val (current, runs) = latest.value
        when {
            current.runId != null || current.stepId != null -> store.showWorkflow(current.id, null)
            runs -> runsOn = false
            current.id != null -> store.showWorkflow(null)
            else -> store.closeWorkflows()
        }
    }
    val pages = buildList {
        add(WorkflowPage.List)
        val id = route.id ?: return@buildList
        add(WorkflowPage.Detail(id))
        if (runsOn) add(WorkflowPage.Runs(id))
        route.runId?.let { add(WorkflowPage.Run(id, it)) }
        route.stepId?.let { add(WorkflowPage.Step(id, it)) }
    }
    NavStack(pages, key = { it.key }, pop = { back.value() }, Modifier.fillMaxSize()) { page ->
        when (page) {
            WorkflowPage.List -> WorkflowListView(app, account, state, store, computers, bottomInset, selectComputer)
            is WorkflowPage.Detail -> WorkflowDetailView(state, store, page.id, { back.value() }) { runsOn = true }
            is WorkflowPage.Runs -> WorkflowRunsView(state, store, page.id, { back.value() }) { store.showWorkflow(page.id, it) }
            is WorkflowPage.Run -> WorkflowRunView(state, store, page.id, page.runId) { back.value() }
            is WorkflowPage.Step -> WorkflowStepView(state, store, page.stepId) { back.value() }
        }
    }
}

private sealed class WorkflowPage {
    data object List : WorkflowPage()
    data class Detail(val id: String) : WorkflowPage()
    data class Runs(val id: String) : WorkflowPage()
    data class Run(val id: String, val runId: String) : WorkflowPage()
    data class Step(val id: String, val stepId: String) : WorkflowPage()
    val key: String get() = when (this) {
        List -> "list"
        is Detail -> "detail:$id"
        is Runs -> "runs:$id"
        is Run -> "run:$runId"
        is Step -> "step:$stepId"
    }
}

@Composable private fun WorkflowListView(app: AccountContext, account: () -> Unit, state: AppState, store: AppStore, computers: List<AppState>, bottomInset: Dp,
    selectComputer: (String) -> Unit) {
    val context = androidx.compose.ui.platform.LocalContext.current
    var pinRevision by remember { mutableIntStateOf(0) }
    var loadError by remember { mutableStateOf<String?>(null) }
    // The search field is open, and what it finds workflows by (title or id).
    var searching by remember { mutableStateOf(false) }
    var query by remember { mutableStateOf("") }
    val shown = remember(state.workflows, query) { workflowMatches(state.workflows, query) }
    val pinned = remember(pinRevision, state.workflows) { WorkflowPins.load(context, state.computer.id).toSet() }
    LaunchedEffect(state.actualConnection, state.offersWorkflows) {
        if (state.actualConnection is LinkState.Ready && state.offersWorkflows) {
            try { store.refreshOpenWorkflows(); loadError = null }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (error: Exception) { loadError = error.message }
        }
    }
    // The Bots tab's stage: the list scrolls under the glass header, softly blurred at the top.
    val top = WindowInsets.statusBars.asPaddingValues().calculateTopPadding()
    GlassStage(Modifier.fillMaxSize().background(Palette.background), chrome = {
        ScrollEdge(Edge.Top, top + 64.dp)
        // The Bots tab's header: the title over the computer shown, then the computer to show and Search; no New yet.
        ListHeader(app, account, "Workflows", state.hostName, searching, query, { query = it }, { searching = false; query = "" },
            Modifier.statusBarsPadding()) {
            if (computers.size > 1) DropdownMenu({
                computers.map { other ->
                    MenuItem(other.hostName, selected = other.computer.id == state.computer.id, verbatim = true) { selectComputer(other.computer.id) }
                }
            }, Modifier.size(44.dp).glass(Capsule()), label = "Computer: ${state.hostName}") {
                Symbol("line.3.horizontal.decrease", size = 24.dp, tint = Palette.text)
            }
            HeaderButton("magnifyingglass", "Search") { searching = true }
        }
    }) {
        Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 20.dp).padding(top = top + 64.dp, bottom = bottomInset + 8.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp)) {
            state.workflowMessage?.let { Text(tr(it), style = Type.callout, color = Palette.danger) }
            loadError?.let { Text(tr(it), style = Type.caption, color = Palette.danger) }
            if (state.workflows.isEmpty()) {
                Column(Modifier.fillMaxWidth().padding(top = 48.dp), horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    Text(tr("No workflows yet"), style = Type.headline, color = Palette.text)
                }
            }
            if (state.workflows.isNotEmpty() && shown.isEmpty()) Text(tr("No results"), Modifier.padding(top = 8.dp), style = Type.callout, color = Palette.secondary)
            if (shown.isNotEmpty()) CardSection(footer = "Hold a workflow to pin it on the bot list.") {
                for (workflow in shown) row {
                    WorkflowSummaryRow(workflow, workflow.id in pinned, { store.showWorkflow(workflow.id) }, inCard = true) {
                        val computer = state.computer.id
                        WorkflowPins.toggle(context, computer, workflow.id)
                        pinRevision++
                    }
                }
            }
        }
    }
}

@Composable internal fun PinnedWorkflowRow(workflow: WorkflowSummary, pinned: Boolean, onOpen: () -> Unit, onTogglePin: () -> Unit) {
    WorkflowSummaryRow(workflow, pinned, onOpen, onTogglePin = onTogglePin)
}

@Composable private fun WorkflowSummaryRow(workflow: WorkflowSummary, pinned: Boolean, onOpen: () -> Unit, inCard: Boolean = false, onTogglePin: () -> Unit) {
    val status = workflow.statusLabel()
    val ended = workflow.lastRun?.endedAt
    val line = when {
        status == null -> tr("Never run")
        workflow.running != null || ended == null -> tr(status)
        else -> "${tr(status)} · ${DateFormat.getDateInstance(DateFormat.MEDIUM).format(Date(ended))}"
    }
    val tint = when (status) {
        "Failed" -> Palette.danger
        "Needs you" -> Palette.attention
        else -> Palette.secondary
    }
    Row(Modifier.fillMaxWidth().contextActions(onClick = onOpen, items = {
        listOf(MenuItem(if (pinned) "Unpin" else "Pin", icon = if (pinned) "pin.slash" else "pin", action = onTogglePin))
    }).then(if (inCard) Modifier else Modifier.padding(vertical = 8.dp)), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.size(32.dp).background(if (inCard) Palette.background else Palette.surface, RoundedCornerShape(9.dp)), contentAlignment = Alignment.Center) {
            Symbol("workflow", size = 16.dp, tint = Palette.text)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(workflow.title.ifBlank { tr("Workflow") }, style = Type.body, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            WorkflowIdLine(workflow.id)
            Text(line, style = Type.footnote, color = tint, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Symbol("chevron.right", size = 14.dp, tint = Palette.tertiary)
    }
}

/** The workflow's own id, so two workflows with the same name stay distinct. */
@Composable internal fun WorkflowIdLine(id: String) {
    if (id.isBlank()) return
    Text(id.take(8), Modifier.fillMaxWidth(), style = Type.footnote.monospaced, color = Palette.tertiary, maxLines = 1)
}

@Composable internal fun WorkflowScreen(title: String, back: () -> Unit, trailing: @Composable androidx.compose.foundation.layout.RowScope.() -> Unit = {},
    content: @Composable androidx.compose.foundation.layout.ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxSize().background(Palette.background).statusBarsPadding().navigationBarsPadding()) {
        PageHeader(title, pop = back, trailing = trailing)
        content()
    }
}

/** A name the user typed, shown as they typed it. */
@Composable internal fun WorkflowTitle(title: String, back: () -> Unit, id: String = "", trailing: @Composable androidx.compose.foundation.layout.RowScope.() -> Unit = {},
    content: @Composable androidx.compose.foundation.layout.ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxSize().background(Palette.background).statusBarsPadding().navigationBarsPadding()) {
        ScreenHeader({ BackButton(back) }, {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(title.ifBlank { tr("Workflow") }, Modifier.widthIn(max = 160.dp), style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
                val short = id.take(8)
                if (short.isNotBlank()) Text(short, style = Type.caption.monospaced, color = Palette.tertiary, maxLines = 1)
            }
        }, trailing)
        content()
    }
}

internal fun AppState.workflowBotName(id: String?): String? =
    id?.let { wanted -> bots.firstOrNull { it.id == wanted && !it.deleted }?.name }

internal fun workflowStamp(milliseconds: Long): String =
    DateFormat.getDateTimeInstance(DateFormat.MEDIUM, DateFormat.SHORT).format(Date(milliseconds))

internal fun workflowDuration(startedAt: Long, endedAt: Long): String {
    val seconds = maxOf(0L, (endedAt - startedAt) / 1000)
    val minutes = seconds / 60
    return if (minutes > 0) "${minutes}m ${seconds % 60}s" else "${seconds}s"
}

@Composable internal fun workflowRunLine(status: String, startedAt: Long, endedAt: Long?): String {
    val label = com.codync.android.core.runStatusLabel(status)
    val ended = endedAt ?: return tr(label)
    return tr("{status} · {time} · {duration}")
        .replace("{status}", tr(label))
        .replace("{time}", workflowStamp(ended))
        .replace("{duration}", workflowDuration(startedAt, ended))
}
