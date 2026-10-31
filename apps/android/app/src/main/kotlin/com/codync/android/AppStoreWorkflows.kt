package com.codync.android

import android.content.Context
import androidx.lifecycle.viewModelScope
import com.codync.android.core.*
import java.util.UUID
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.encodeToJsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/** The graph the editor has not saved yet. */
internal data class HeldWorkflow(val id: String, val revision: Long, val graph: Graph)

private val workflowSaves = Mutex()

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

internal fun AppStore.holdWorkflow(id: String, revision: Long, graph: Graph) {
    heldWorkflow = HeldWorkflow(id, revision, graph)
    dirtyWorkflows.add(id)
}

internal fun AppStore.releaseWorkflow(id: String) {
    if (heldWorkflow?.id == id) heldWorkflow = null
    dirtyWorkflows.remove(id)
}

internal fun AppStore.isWorkflowDirty(id: String): Boolean = id in dirtyWorkflows

/** Opens the list and leaves the chat. */
fun AppStore.openWorkflows() {
    mutableState.update { it.copy(selectedBot = null, workflow = WorkflowRoute(), workflowMessage = null) }
}

/** Opens one workflow, and a run when [runId] is set. Clears the open chat. */
fun AppStore.showWorkflow(id: String?, runId: String? = null) {
    mutableState.update {
        it.copy(selectedBot = null, workflow = if (id == null) WorkflowRoute() else WorkflowRoute(id, runId), workflowMessage = null)
    }
}

fun AppStore.closeWorkflows() {
    mutableState.update { it.copy(workflow = null) }
}

internal fun AppStore.noteWorkflow(message: String?) {
    mutableState.update { it.copy(workflowMessage = message) }
}

/** A notification that names a workflow: leave the chat, land on Bots, open the workflow or its run. */
fun AccountStore.openWorkflowLink(uri: android.net.Uri, workflowId: String, runId: String?) {
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
            openBot(null)
            mutableState.update { it.copy(error = null, navigationRequest = it.navigationRequest + 1, navigationPage = LinkPage.Bots) }
            store.showWorkflow(workflowId, runId?.takeIf { id -> id.isNotBlank() })
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

/** Live workflow events. Enabled, run, and step refresh the summary and an open run, not a dirty graph. */
internal fun AppStore.receiveWorkflows(events: List<MirrorEvent>) {
    val notes = events.filterIsInstance<MirrorEvent.Workflow>().map { it.change }
    if (notes.isEmpty() || !state.value.offersWorkflows) return
    val deleted = notes.lastOrNull { it.change == "deleted" }
    if (deleted != null) {
        WorkflowPins.forget(application, state.value.computer.id, deleted.workflowId)
        releaseWorkflow(deleted.workflowId)
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
    for (note in notes) {
        val enabled = note.enabled
        if (note.change == "enabled" && enabled != null) {
            mutableState.update { state ->
                state.copy(workflows = state.workflows.map { row -> if (row.id == note.workflowId) row.copy(enabled = enabled) else row })
            }
        }
    }
    if (notes.any { it.change != "deleted" }) {
        mutableState.update { it.copy(workflowTick = it.workflowTick + 1) }
        contextScope.launch {
            try { refreshWorkflows() }
            catch (cancelled: CancellationException) { throw cancelled }
            catch (_: Exception) { }
        }
    }
}

internal suspend fun AppStore.fetchWorkflow(id: String, reveal: Boolean = false): WorkflowDetail {
    val response = ready().call("workflow", buildJsonObject { put("id", id); put("reveal", reveal) })
    return WireJson.decodeFromJsonElement(response.jsonObject.getValue("workflow"))
}

internal suspend fun AppStore.saveWorkflowGraph(id: String, baseRevision: Long, graph: Graph): WorkflowSaveResult = workflowSaves.withLock {
    val response = ready().call("saveWorkflowGraph", buildJsonObject {
        put("id", id)
        put("baseRevision", baseRevision)
        put("graph", WireJson.encodeToJsonElement(Graph.serializer(), graph))
    })
    WireJson.decodeFromJsonElement(response)
}

/** Saves a held graph before a run. A conflict replaces nothing here; the editor resets from the result. */
internal suspend fun AppStore.flushHeldWorkflow(): WorkflowSaveResult? {
    val held = heldWorkflow ?: return null
    if (held.id !in dirtyWorkflows) return null
    val result = saveWorkflowGraph(held.id, held.revision, held.graph)
    if (result.conflict) {
        releaseWorkflow(held.id)
        mutableState.update { it.copy(workflowMessage = "This workflow was edited somewhere else", workflowTick = it.workflowTick + 1) }
    } else releaseWorkflow(held.id)
    return result
}

fun AppStore.createWorkflow(created: (WorkflowDetail) -> Unit) = action("workflow") {
    val response = ready().call("createWorkflow", buildJsonObject { put("title", "Workflow") })
    val detail = WireJson.decodeFromJsonElement<WorkflowDetail>(response.jsonObject.getValue("workflow"))
    mutableState.update {
        it.copy(workflows = (listOf(detail.summary()) + it.workflows.filter { row -> row.id != detail.id }).listed(),
            selectedBot = null, workflow = WorkflowRoute(detail.id))
    }
    created(detail)
}

fun AppStore.renameWorkflow(id: String, title: String) = action("workflow") {
    val response = ready().call("renameWorkflow", buildJsonObject { put("id", id); put("title", title) })
    val summary = WireJson.decodeFromJsonElement<WorkflowSummary>(response.jsonObject.getValue("workflow"))
    mutableState.update { it.copy(workflows = it.workflows.map { row -> if (row.id == id) summary else row }.listed(), workflowTick = it.workflowTick + 1) }
}

fun AppStore.deleteWorkflow(id: String) = action("workflow") {
    ready().call("deleteWorkflow", buildJsonObject { put("id", id) })
    WorkflowPins.forget(application, state.value.computer.id, id)
    releaseWorkflow(id)
    mutableState.update {
        it.copy(workflows = it.workflows.filter { row -> row.id != id }, workflow = WorkflowRoute(), workflowMessage = null, workflowTick = it.workflowTick + 1)
    }
}

fun AppStore.setWorkflowEnabled(id: String, enabled: Boolean) = action("workflow") {
    val response = ready().call("setWorkflowEnabled", buildJsonObject { put("id", id); put("enabled", enabled) })
    val summary = WireJson.decodeFromJsonElement<WorkflowSummary>(response.jsonObject.getValue("workflow"))
    mutableState.update { it.copy(workflows = it.workflows.map { row -> if (row.id == id) summary else row }, workflowTick = it.workflowTick + 1) }
}

internal suspend fun AppStore.workflowVersions(id: String): List<WorkflowVersion> {
    val response = ready().call("workflowVersions", buildJsonObject { put("id", id) })
    return WireJson.decodeFromJsonElement<WorkflowVersionList>(response).versions
}

internal suspend fun AppStore.fetchWorkflowVersion(id: String, versionId: String): Graph {
    val response = ready().call("workflowVersion", buildJsonObject { put("id", id); put("versionId", versionId) })
    return WireJson.decodeFromJsonElement(response.jsonObject.getValue("graph"))
}

internal suspend fun AppStore.restoreWorkflowVersion(id: String, versionId: String): WorkflowDetail {
    val response = ready().call("restoreWorkflowVersion", buildJsonObject { put("id", id); put("versionId", versionId) })
    releaseWorkflow(id)
    val detail = WireJson.decodeFromJsonElement<WorkflowDetail>(response.jsonObject.getValue("workflow"))
    mutableState.update { it.copy(workflowTick = it.workflowTick + 1) }
    return detail
}

internal suspend fun AppStore.startWorkflowRun(id: String, fromNodeId: String? = null): WorkflowRunDetail {
    flushHeldWorkflow()?.let { result ->
        if (result.conflict) throw IllegalStateException("This workflow was edited somewhere else")
    }
    val response = ready().call("startWorkflowRun", buildJsonObject {
        put("id", id)
        put("requestId", UUID.randomUUID().toString())
        if (fromNodeId != null) put("fromNodeId", fromNodeId)
    })
    return WireJson.decodeFromJsonElement(response.jsonObject.getValue("run"))
}

internal suspend fun AppStore.stopWorkflowRun(runId: String) {
    ready().call("stopWorkflowRun", buildJsonObject { put("runId", runId) })
}

internal suspend fun AppStore.runWorkflowStep(id: String, nodeId: String): WorkflowStep {
    flushHeldWorkflow()?.let { result ->
        if (result.conflict) throw IllegalStateException("This workflow was edited somewhere else")
    }
    val response = ready().call("runWorkflowStep", buildJsonObject {
        put("id", id); put("nodeId", nodeId); put("requestId", UUID.randomUUID().toString())
    })
    return WireJson.decodeFromJsonElement(response.jsonObject.getValue("step"))
}

internal suspend fun AppStore.stopWorkflowStep(stepId: String) {
    ready().call("stopWorkflowStep", buildJsonObject { put("stepId", stepId) })
}

internal suspend fun AppStore.retryWorkflowStep(stepId: String): WorkflowStep {
    val response = ready().call("retryWorkflowStep", buildJsonObject { put("stepId", stepId) })
    return WireJson.decodeFromJsonElement(response.jsonObject.getValue("step"))
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

internal suspend fun AppStore.fetchWorkflowRun(runId: String): WorkflowRunDetail {
    val response = ready().call("workflowRun", buildJsonObject { put("runId", runId) })
    return WireJson.decodeFromJsonElement(response.jsonObject.getValue("run"))
}

internal suspend fun AppStore.rotateWorkflowWebhook(id: String, nodeId: String): String {
    val response = ready().call("rotateWorkflowWebhook", buildJsonObject { put("id", id); put("nodeId", nodeId) })
    return response.jsonObject.getValue("secret").jsonPrimitive.content
}
