package com.codync.android

import android.content.Context
import androidx.lifecycle.viewModelScope
import com.codync.android.core.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/** Pinned workflows for this computer, in pin order, when hello offered them. */
internal fun pinnedWorkflows(context: Context, state: AppState): List<WorkflowSummary> {
    if (!state.offersWorkflows) return emptyList()
    return WorkflowPins.load(context, state.computer.id).mapNotNull { id -> state.workflows.firstOrNull { it.id == id } }
}

/** Pinned workflow ids for one computer, on this phone only. At most 20. */
internal object WorkflowPins {
    private const val FILE = "codync-workflows"
    fun load(context: Context, computerId: String): List<String> {
        val raw = context.getSharedPreferences(FILE, Context.MODE_PRIVATE).getString(key(computerId), null) ?: return emptyList()
        return runCatching { WireJson.decodeFromString<List<String>>(raw) }.getOrDefault(emptyList())
    }
    fun toggle(context: Context, computerId: String, id: String): List<String> {
        val current = load(context, computerId)
        val next = if (id in current) current.filter { it != id } else (listOf(id) + current).take(20)
        save(context, computerId, next)
        return next
    }
    fun forget(context: Context, computerId: String, id: String) {
        save(context, computerId, load(context, computerId).filter { it != id })
    }
    private fun save(context: Context, computerId: String, ids: List<String>) {
        context.getSharedPreferences(FILE, Context.MODE_PRIVATE).edit().putString(key(computerId), WireJson.encodeToString(ids)).apply()
    }
    private fun key(computerId: String) = "pinnedWorkflows.$computerId"
}

/** Opens the list and leaves the chat. */
fun AppStore.openWorkflows() {
    mutableState.update { it.copy(selectedBot = null, workflow = WorkflowRoute(), workflowMessage = null) }
}

/** Opens one workflow, and a run when [runId] is set (or a step run on its own, [stepId]). Clears the open chat. */
fun AppStore.showWorkflow(id: String?, runId: String? = null, stepId: String? = null) {
    mutableState.update { it.copy(selectedBot = null, workflow = workflowRoute(id, runId, stepId), workflowMessage = null) }
}

/** Where a workflow opens: a step counts only outside a run (a step's push names its run, or only the step when it ran
 *  on its own); blank ids are none. */
internal fun workflowRoute(id: String?, runId: String?, stepId: String?): WorkflowRoute {
    if (id == null) return WorkflowRoute()
    val run = runId?.takeIf { it.isNotBlank() }
    return WorkflowRoute(id, run, stepId?.takeIf { it.isNotBlank() && run == null })
}

fun AppStore.closeWorkflows() {
    mutableState.update { it.copy(workflow = null) }
}

/** A notification that names a workflow: leave the chat, land on Bots, open the workflow, its run or a step run on its own. */
fun AccountStore.openWorkflowLink(uri: android.net.Uri, workflowId: String, runId: String?, stepId: String? = null) {
    linkJob?.cancel()
    linkJob = viewModelScope.launch {
        val completed = withTimeoutOrNull(30_000) {
            val ready = activated()
            val destination = try { BotDestination.parse(uri.toString(), BuildConfig.URL_SCHEME) }
            catch (_: Exception) {
                mutableState.update { it.copy(error = "This notification has an invalid destination.") }
                return@withTimeoutOrNull true
            }
            if (destination.scope != ready.contextId) {
                mutableState.update { it.copy(error = "Switch to the account used by this notification to open its bot.") }
                return@withTimeoutOrNull true
            }
            val store = store(destination.computerId)
            if (store == null) {
                mutableState.update { it.copy(error = "This notification's computer is no longer paired here.") }
                return@withTimeoutOrNull true
            }
            if (state.value.generation != ready.generation) return@withTimeoutOrNull true
            focusWorkflows(destination.computerId, workflowId, runId, stepId)
            true
        }
        if (completed == null) mutableState.update { it.copy(error = "Couldn't open this notification yet. Reconnect and try again.") }
    }
}

internal suspend fun AppStore.refreshWorkflows() {
    if (state.value.hello?.get("workflows")?.jsonPrimitive?.booleanOrNull != true) return
    try {
        val response = ready().call("workflows", buildJsonObject { })
        val list = WireJson.decodeFromJsonElement<WorkflowList>(response).workflows.listed()
        mutableState.update { it.copy(workflows = list, workflowsSupported = true) }
    } catch (cancelled: CancellationException) { throw cancelled }
    catch (error: HostError) {
        if (error.status == 404) mutableState.update {
            it.copy(workflowsSupported = false, workflows = emptyList(), workflow = null, workflowMessage = error.message)
        } else throw error
    }
}

/** Live workflow events: a deletion closes it here; any other change reloads the list and an open detail or run. */
internal fun AppStore.receiveWorkflows(events: List<MirrorEvent>) {
    val notes = events.filterIsInstance<MirrorEvent.Workflow>().map { it.change }
    if (notes.isEmpty() || !state.value.offersWorkflows) return
    val deleted = notes.lastOrNull { it.change == "deleted" }
    if (deleted != null) {
        WorkflowPins.forget(application, state.value.computer.id, deleted.workflowId)
        mutableState.update {
            val open = it.workflow?.id == deleted.workflowId
            it.copy(
                workflows = it.workflows.filter { row -> row.id != deleted.workflowId },
                workflow = if (open) WorkflowRoute() else it.workflow,
                workflowMessage = if (open) "This workflow was deleted" else it.workflowMessage,
                workflowTick = it.workflowTick + 1,
            )
        }
    }
    if (notes.any { it.change != "deleted" }) {
        refreshOpenWorkflows()
    }
}

/** Reconnects and live workflow events both reload summaries and whatever detail or run the phone has open. */
internal fun AppStore.refreshOpenWorkflows() {
    mutableState.update { it.copy(workflowTick = it.workflowTick + 1) }
    contextScope.launch {
        try { refreshWorkflows() }
        catch (cancelled: CancellationException) { throw cancelled }
        catch (_: Exception) { }
    }
}

internal suspend fun AppStore.fetchWorkflow(id: String): WorkflowDetail {
    val response = ready().call("workflow", buildJsonObject { put("id", id) })
    return WireJson.decodeFromJsonElement(response.jsonObject.getValue("workflow"))
}

internal suspend fun AppStore.decideWorkflowStep(stepId: String, approved: Boolean) {
    ready().call("decideWorkflowStep", buildJsonObject { put("stepId", stepId); put("approved", approved) })
}

internal suspend fun AppStore.workflowRuns(id: String, limit: Int, before: WorkflowRunSummary? = null): WorkflowRunPage {
    val response = ready().call("workflowRuns", buildJsonObject {
        put("id", id)
        put("limit", limit)
        if (before != null) put("before", buildJsonObject { put("startedAt", before.startedAt); put("id", before.id) })
    })
    return WireJson.decodeFromJsonElement(response)
}

/** One step run on its own, outside any run. */
internal suspend fun AppStore.fetchWorkflowStep(stepId: String): WorkflowStep {
    val response = ready().call("workflowStep", buildJsonObject { put("stepId", stepId) })
    return WireJson.decodeFromJsonElement(response.jsonObject.getValue("step"))
}

internal suspend fun AppStore.fetchWorkflowRun(runId: String): WorkflowRunDetail {
    val response = ready().call("workflowRun", buildJsonObject { put("runId", runId) })
    return WireJson.decodeFromJsonElement(response.jsonObject.getValue("run"))
}
