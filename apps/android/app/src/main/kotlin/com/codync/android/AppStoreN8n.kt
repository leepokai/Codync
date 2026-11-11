package com.codync.android

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import java.time.Instant

/** The computer's n8n as the host reports it (`n8nStatus`): never the editor's session cookie, which a phone isn't given. */
internal data class N8nStatus(
    /** `stopped`, `starting`, `running` or `error`. */
    val state: String,
    val error: String?,
    val enabled: Boolean,
    /** Where n8n's own folder is; null until the computer's app has chosen it. */
    val path: String?,
) {
    val active get() = state == "running" || state == "starting"
    val running get() = state == "running"

    /** The line under the title (a catalog key). */
    val stateLabel get() = when (state) {
        "running" -> "Running"
        "starting" -> "Starting {name}… the first start can take a few minutes."
        "error" -> "Stopped with an error"
        else -> "Stopped"
    }

    companion object {
        fun decode(json: JsonObject) = N8nStatus(
            state = json.text("state") ?: "stopped",
            error = json.text("error"),
            enabled = (json["enabled"] as? JsonPrimitive)?.booleanOrNull ?: false,
            path = json.text("path"),
        )
    }
}

/** One workflow: whether it is published (switched on) in n8n. */
internal data class N8nWorkflow(val id: String, val name: String, val active: Boolean)

/** One run of a workflow. */
internal data class N8nRun(val id: String, val workflowId: String?, val status: String, val startedAtMillis: Long?) {
    /** The run's status as a catalog key. */
    val statusLabel get() = when (status) {
        "success" -> "Succeeded"
        "error" -> "Failed"
        "crashed" -> "Crashed"
        "running" -> "Running"
        "waiting" -> "Waiting"
        "canceled" -> "Cancelled"
        "new" -> "Waiting in the queue"
        else -> "Unknown"
    }
}

/** The workflows list with how many more n8n has than the page the host fetched. */
internal data class N8nWorkflows(val workflows: List<N8nWorkflow>, val more: Boolean) {
    companion object {
        fun decode(json: JsonObject) = N8nWorkflows(
            workflows = (json["workflows"] as? JsonArray).orEmpty().mapNotNull { row ->
                val w = row as? JsonObject ?: return@mapNotNull null
                val id = w.text("id") ?: return@mapNotNull null
                N8nWorkflow(id, w.text("name").orEmpty(), (w["active"] as? JsonPrimitive)?.booleanOrNull ?: false)
            },
            more = (json["more"] as? JsonPrimitive)?.booleanOrNull ?: false,
        )
    }
}

internal fun decodeN8nRuns(json: JsonObject): List<N8nRun> = (json["executions"] as? JsonArray).orEmpty().mapNotNull { row ->
    val e = row as? JsonObject ?: return@mapNotNull null
    val id = e.text("id") ?: return@mapNotNull null
    N8nRun(id, e.text("workflowId"), e.text("status") ?: "unknown",
        e.text("startedAt")?.let { runCatching { Instant.parse(it).toEpochMilli() }.getOrNull() })
}

/** A string field, or null when it is missing, null or not text. */
private fun JsonObject.text(key: String): String? = (this[key] as? JsonPrimitive)?.takeIf { it.isString }?.content

internal suspend fun AppStore.n8nStatus(): N8nStatus = N8nStatus.decode(hostQuery("n8nStatus", buildJsonObject {}).jsonObject)

internal suspend fun AppStore.n8nStart(): N8nStatus = N8nStatus.decode(hostQuery("n8nStart", buildJsonObject {}).jsonObject)

internal suspend fun AppStore.n8nStop(): N8nStatus = N8nStatus.decode(hostQuery("n8nStop", buildJsonObject {}).jsonObject)

internal suspend fun AppStore.n8nWorkflows(): N8nWorkflows = N8nWorkflows.decode(hostQuery("n8nWorkflows", buildJsonObject {}).jsonObject)

/** Publishes or unpublishes one workflow on the computer. */
internal suspend fun AppStore.n8nSetWorkflowActive(id: String, active: Boolean) {
    hostQuery("n8nSetWorkflowActive", buildJsonObject { put("id", id); put("active", active) })
}

internal suspend fun AppStore.n8nRuns(limit: Int = 5): List<N8nRun> =
    decodeN8nRuns(hostQuery("n8nExecutions", buildJsonObject { put("limit", limit) }).jsonObject)
