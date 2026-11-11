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
    /** Whether its program is on the computer; the first start downloads it. */
    val installed: Boolean = true,
    /** The download's size, for the line that offers it. */
    val downloadBytes: Long = 0,
    /** `idle`, `downloading`, `unpacking` or `failed`. */
    val installPhase: String = "idle",
    val installDone: Long = 0,
    val installTotal: Long = 0,
    val installError: String? = null,
) {
    val active get() = state == "running" || state == "starting"
    /** The program is arriving: Start waits for it. */
    val installing get() = installPhase == "downloading" || installPhase == "unpacking"
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
            fun number(value: Any?) = (value as? kotlinx.serialization.json.JsonPrimitive)?.content?.toLongOrNull() ?: 0L
            val card = json["comfy"] as? JsonObject
            val job = json["job"] as? JsonObject
            val install = json["install"] as? JsonObject
            return ComfyStatus(
                state = text(json["state"]) ?: "stopped",
                error = text(json["error"]),
                bundled = card?.get("bundled")?.jsonPrimitive?.content != "false",
                notice = text(card?.get("notice")),
                jobPhase = text(job?.get("phase")),
                // An older host has no download: its ComfyUI is either there or not offered.
                installed = card?.get("installed")?.jsonPrimitive?.content != "false",
                downloadBytes = number(card?.get("downloadBytes")),
                installPhase = text(install?.get("phase")) ?: "idle",
                installDone = number(install?.get("done")),
                installTotal = number(install?.get("total")),
                installError = text(install?.get("error")),
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
