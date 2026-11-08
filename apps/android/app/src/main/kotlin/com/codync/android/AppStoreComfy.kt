package com.codync.android

import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/** The computer's ComfyUI as the host reports it (`toolStatus`), in the words the screen shows. */
internal data class ComfyStatus(
    /** `stopped`, `starting`, `running` or `error`. */
    val state: String,
    val error: String?,
    /** False on a computer whose Codync doesn't include ComfyUI. */
    val bundled: Boolean,
    val notice: String?,
    /** `queued`, `running`, `done`, `cancelled` or `error`; null before the first job. */
    val jobPhase: String?,
) {
    val active get() = state == "running" || state == "starting"
    val jobActive get() = jobPhase == "queued" || jobPhase == "running"

    /** The line under the logo (a catalog key). */
    val stateLabel get() = when (state) {
        "running" -> "Running"
        "starting" -> "Starting {name}… the first start can take a few minutes."
        "error" -> "Stopped with an error"
        else -> "Stopped"
    }

    /** The job's line (a catalog key), or null before the first job. */
    val jobLabel get() = when (jobPhase) {
        "queued" -> "Waiting in the queue"
        "running" -> "Making an image…"
        "done" -> "Done"
        "cancelled" -> "Cancelled"
        "error" -> "Failed"
        else -> null
    }

    companion object {
        fun decode(json: JsonObject): ComfyStatus {
            fun text(value: Any?) = (value as? kotlinx.serialization.json.JsonPrimitive)?.takeIf { it.isString }?.content
            val card = json["comfy"] as? JsonObject
            val job = json["job"] as? JsonObject
            return ComfyStatus(
                state = text(json["state"]) ?: "stopped",
                error = text(json["error"]),
                bundled = card?.get("bundled")?.jsonPrimitive?.content != "false",
                notice = text(card?.get("notice")),
                jobPhase = text(job?.get("phase")),
            )
        }
    }
}

private fun comfy() = buildJsonObject { put("id", "comfy") }

internal suspend fun AppStore.comfyStatus(): ComfyStatus = ComfyStatus.decode(hostQuery("toolStatus", comfy()).jsonObject)

internal suspend fun AppStore.comfyStart(): ComfyStatus = ComfyStatus.decode(hostQuery("toolStart", comfy()).jsonObject)

internal suspend fun AppStore.comfyStop(): ComfyStatus = ComfyStatus.decode(hostQuery("toolStop", comfy()).jsonObject)

/** Stops the job ComfyUI is running for a bot; the section asks for the status again. */
internal suspend fun AppStore.comfyCancel() { hostQuery("comfyCancel", buildJsonObject {}) }
