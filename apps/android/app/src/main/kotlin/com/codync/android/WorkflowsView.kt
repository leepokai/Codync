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
import androidx.compose.foundation.layout.statusBarsPadding
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
import androidx.compose.ui.unit.dp
import com.codync.android.core.Bot
import com.codync.android.core.LinkState
import com.codync.android.core.WorkflowSummary
import com.codync.android.core.statusLabel
import com.codync.android.design.*
import java.util.Date
import kotlinx.coroutines.CancellationException

/** Workflows between the roster and a chat: list, one workflow, its editor, its runs, or one run. */
@Composable internal fun WorkflowsView(state: AppState, store: AppStore) {
    val route = state.workflow ?: return
    var editOn by rememberSaveable { mutableStateOf<String?>(null) }
    var runsOn by rememberSaveable { mutableStateOf(false) }
    if (editOn != null && editOn != route.id) editOn = null
    val back = remember { mutableStateOf<() -> Unit>({}) }
    val latest = rememberUpdatedState(Triple(route, editOn, runsOn))
    back.value = {
        val (current, editing, runs) = latest.value
        when {
            editing != null && editing == current.id -> editOn = null
            current.runId != null -> store.showWorkflow(current.id, null)
            runs -> runsOn = false
            current.id != null -> store.showWorkflow(null)
            else -> store.closeWorkflows()
        }
    }
    val pages = buildList {
        add(WorkflowPage.List)
        val id = route.id ?: return@buildList
        add(WorkflowPage.Detail(id))
        if (editOn == id) add(WorkflowPage.Editor(id))
        else {
            if (runsOn) add(WorkflowPage.Runs(id))
            route.runId?.let { add(WorkflowPage.Run(id, it)) }
        }
    }
    NavStack(pages, key = { it.key }, pop = { back.value() }, Modifier.fillMaxSize()) { page ->
        when (page) {
            WorkflowPage.List -> WorkflowListView(state, store, { back.value() }) { editOn = it }
            is WorkflowPage.Detail -> WorkflowDetailView(state, store, page.id, { back.value() }, { editOn = page.id }, { runsOn = true })
            is WorkflowPage.Editor -> WorkflowEditorView(state, store, page.id) { back.value() }
            is WorkflowPage.Runs -> WorkflowRunsView(state, store, page.id, { back.value() }) { store.showWorkflow(page.id, it) }
            is WorkflowPage.Run -> WorkflowRunView(state, store, page.id, page.runId) { back.value() }
        }
    }
}

private sealed class WorkflowPage {
    data object List : WorkflowPage()
    data class Detail(val id: String) : WorkflowPage()
    data class Editor(val id: String) : WorkflowPage()
    data class Runs(val id: String) : WorkflowPage()
    data class Run(val id: String, val runId: String) : WorkflowPage()
    val key: String get() = when (this) {
        List -> "list"
        is Detail -> "detail:$id"
        is Editor -> "editor:$id"
        is Runs -> "runs:$id"
        is Run -> "run:$runId"
    }
}

@Composable private fun WorkflowListView(state: AppState, store: AppStore, back: () -> Unit, created: (String) -> Unit) {
    AnalyticsOnAppear(AnalyticsEvent.ScreenViewed(AnalyticsScreen.Workflows))
    val context = androidx.compose.ui.platform.LocalContext.current
    var pinRevision by remember { mutableIntStateOf(0) }
    var loadError by remember { mutableStateOf<String?>(null) }
    val pinned = remember(pinRevision, state.workflows) { WorkflowPins.load(context, state.computer.id).toSet() }
    LaunchedEffect(state.actualConnection, state.offersWorkflows) {
        if (state.actualConnection is LinkState.Ready && state.offersWorkflows) {
            try { store.refreshWorkflows(); loadError = null }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (error: Exception) { loadError = error.message }
        }
    }
    WorkflowScreen("Workflows", back, trailing = {
        IconButton("New workflow", "plus", enabled = state.actualConnection is LinkState.Ready) {
            store.createWorkflow { created(it.id) }
        }
    }) {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = 20.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp)) {
            state.workflowMessage?.let { Text(tr(it), style = Type.callout, color = Palette.danger) }
            loadError?.let { Text(tr(it), style = Type.caption, color = Palette.danger) }
            if (state.workflows.isEmpty()) {
                Column(Modifier.fillMaxWidth().padding(top = 48.dp), horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(16.dp)) {
                    Text(tr("No workflows yet"), style = Type.headline, color = Palette.text)
                    PrimaryButton("New workflow", enabled = state.actualConnection is LinkState.Ready) { store.createWorkflow { created(it.id) } }
                }
            }
            for (workflow in state.workflows) {
                WorkflowSummaryRow(workflow, workflow.id in pinned, { store.showWorkflow(workflow.id) }) {
                    val computer = state.computer.id
                    WorkflowPins.toggle(context, computer, workflow.id)
                    pinRevision++
                }
            }
        }
    }
}

@Composable internal fun PinnedWorkflowRow(workflow: WorkflowSummary, pinned: Boolean, onOpen: () -> Unit, onTogglePin: () -> Unit) {
    WorkflowSummaryRow(workflow, pinned, onOpen, onTogglePin)
}

@Composable private fun WorkflowSummaryRow(workflow: WorkflowSummary, pinned: Boolean, onOpen: () -> Unit, onTogglePin: () -> Unit) {
    val status = workflow.statusLabel()
    Row(Modifier.fillMaxWidth().contextActions(onClick = onOpen, items = {
        listOf(MenuItem(if (pinned) "Unpin" else "Pin", icon = if (pinned) "pin.slash" else "pin", action = onTogglePin))
    }).padding(vertical = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Symbol("flowchart", size = 22.dp, tint = Palette.text)
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(workflow.title.ifBlank { tr("Workflow") }, style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
            if (status != null) Text(tr(status), style = Type.footnote, color = if (status == "Failed") Palette.danger else if (status == "Needs you") Palette.attention else Palette.secondary)
        }
    }
}

@Composable internal fun WorkflowScreen(title: String, back: () -> Unit, trailing: @Composable androidx.compose.foundation.layout.RowScope.() -> Unit = {},
    content: @Composable androidx.compose.foundation.layout.ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxSize().background(Palette.background).statusBarsPadding().navigationBarsPadding()) {
        PageHeader(title, pop = back, trailing = trailing)
        content()
    }
}

/** A name the user typed, shown as they typed it. */
@Composable internal fun WorkflowTitle(title: String, back: () -> Unit, trailing: @Composable androidx.compose.foundation.layout.RowScope.() -> Unit = {},
    content: @Composable androidx.compose.foundation.layout.ColumnScope.() -> Unit) {
    Column(Modifier.fillMaxSize().background(Palette.background).statusBarsPadding().navigationBarsPadding()) {
        ScreenHeader({ BackButton(back) }, {
            Text(title.ifBlank { tr("Workflow") }, style = Type.body.semibold, color = Palette.text, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }, trailing)
        content()
    }
}

internal fun AppState.workflowBotName(id: String?): String? =
    id?.let { wanted -> bots.firstOrNull { it.id == wanted && !it.deleted }?.name }

internal fun AppState.workflowModel(id: String?): String? {
    val bot = bots.firstOrNull { it.id == id && !it.deleted } ?: return null
    return bot.model?.takeIf { it.isNotBlank() } ?: bot.backend.takeIf { it.isNotBlank() }
}

internal fun AppState.workflowAgents(): List<Bot> = bots.filter { !it.deleted && it.kind != "group" }

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
