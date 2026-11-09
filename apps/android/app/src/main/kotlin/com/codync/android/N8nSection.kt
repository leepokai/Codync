package com.codync.android

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
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
import androidx.compose.ui.unit.dp
import com.codync.android.design.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** How often the section looks at the computer again while it shows. */
private const val REFRESH_MILLIS = 5_000L
/** The most workflows the card lists; the rest are in n8n. */
private const val SHOWN_WORKFLOWS = 25

/**
 * The computer's n8n: whether it runs, Start and Stop, its workflows with a switch to publish each, and its
 * latest runs. Its editor opens in Codync on the computer; the phone steers it, the host does the work.
 */
@Composable internal fun N8nSection(state: AppState, store: AppStore) {
    val scope = rememberCoroutineScope()
    var status by remember { mutableStateOf<N8nStatus?>(null) }
    var workflows by remember { mutableStateOf<N8nWorkflows?>(null) }
    var runs by remember { mutableStateOf<List<N8nRun>>(emptyList()) }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    fun run(block: suspend () -> Unit) = scope.launch {
        busy = true
        try { block(); error = null }
        catch (e: CancellationException) { throw e }
        catch (e: Exception) { error = e.message }
        busy = false
    }
    suspend fun look() {
        val now = store.n8nStatus()
        status = now
        // Workflows and runs exist only while n8n answers.
        if (now.running) {
            workflows = store.n8nWorkflows()
            runs = store.n8nRuns()
        } else {
            workflows = null
            runs = emptyList()
        }
    }
    LaunchedEffect(state.contextId) {
        while (true) {
            try { look(); error = null }
            catch (e: CancellationException) { throw e }
            catch (e: Exception) { error = e.message }
            delay(REFRESH_MILLIS)
        }
    }
    val current = status
    CardSection("n8n", "The workflows n8n runs on the computer. Turning one on publishes it there. Its editor opens in Codync on the computer.") {
        row {
            Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    Symbol("point.3.connected.trianglepath.dotted", size = 20.dp, tint = Palette.text)
                    Text(
                        if (current == null) tr("Checking {name}…").replace("{name}", "n8n")
                        else tr(current.stateLabel).replace("{name}", "n8n"),
                        style = Type.body, color = Palette.text, modifier = Modifier.weight(1f),
                    )
                }
                current?.error?.let { Text(it, style = Type.caption, color = Palette.danger) }
                if (current != null) {
                    if (current.path == null && !current.active) {
                        Text(tr("n8n isn't set up on this computer yet. Choose its folder in the desktop app."),
                            style = Type.caption, color = Palette.secondary)
                    } else if (current.active) {
                        SecondaryButton(tr("Stop"), enabled = !busy) { run { status = store.n8nStop(); look() } }
                    } else {
                        SecondaryButton(tr("Start"), enabled = !busy) { run { status = store.n8nStart() } }
                    }
                }
            }
        }
        val list = workflows
        if (current?.running == true) {
            if (list == null || list.workflows.isEmpty()) {
                row { Text(tr(if (list == null) "Checking {name}…" else "No workflows yet.").replace("{name}", "n8n"),
                    style = Type.caption, color = Palette.secondary) }
            } else {
                list.workflows.take(SHOWN_WORKFLOWS).forEach { workflow ->
                    row {
                        SwitchRow(workflow.name.ifBlank { workflow.id }, isOn = workflow.active, enabled = !busy, onChange = { on ->
                            run { store.n8nSetWorkflowActive(workflow.id, on); look() }
                        })
                    }
                }
                val hidden = list.workflows.size - SHOWN_WORKFLOWS
                if (hidden > 0 || list.more) {
                    row { Text(tr("{count} more in n8n").replace("{count}", if (hidden > 0) "$hidden" else "+"),
                        style = Type.caption, color = Palette.secondary) }
                }
            }
            if (runs.isNotEmpty()) {
                row {
                    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        Text(tr("Latest runs"), style = Type.caption, color = Palette.secondary)
                        runs.forEach { entry ->
                            val name = list?.workflows?.firstOrNull { it.id == entry.workflowId }?.name
                            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                                Text(name ?: entry.workflowId.orEmpty(), style = Type.caption, color = Palette.text,
                                    modifier = Modifier.weight(1f), maxLines = 1)
                                Text(tr(entry.statusLabel), style = Type.caption,
                                    color = if (entry.status == "error" || entry.status == "crashed") Palette.danger else Palette.secondary)
                                entry.startedAtMillis?.let { Text(RelativeTime.short(it), style = Type.caption, color = Palette.secondary) }
                            }
                        }
                    }
                }
            }
        } else if (current != null) {
            row { Text(tr("Start n8n to see its workflows and runs."), style = Type.caption, color = Palette.secondary) }
        }
        error?.let { row { Text(it, style = Type.caption, color = Palette.danger) } }
    }
}
