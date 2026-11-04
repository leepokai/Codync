package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

/** What one agent the host started uses. */
@Serializable data class HealthAgent(val botId: String, val botName: String = "", val pid: Long = 0, val rss: Long = 0)

@Serializable data class HealthLevels(val warn: Int = 70, val critical: Int = 85)

/** The computer's health as the host reports it (`hostHealth`). The host owns the policy; this words it. */
@Serializable data class HostHealth(
    val ramTotal: Long = 0,
    val ramFree: Long = 0,
    val ramUsedPercent: Int = 0,
    val cpu: Double = 0.0,
    val diskTotal: Long = 0,
    val diskFree: Long = 0,
    /** `normal`, `warning` or `critical`. */
    val level: String = "normal",
    /** New turns are not starting. */
    val refusing: Boolean = false,
    val thresholds: HealthLevels = HealthLevels(),
    val agents: List<HealthAgent> = emptyList(),
) {
    companion object {
        fun decode(response: JsonElement): HostHealth = WireJson.decodeFromJsonElement(serializer(), response)

        private const val GB = 1024L * 1024 * 1024
        private const val MB = 1024L * 1024

        /** `1.5 GB`, `320 MB`. */
        fun bytesText(bytes: Long): String =
            if (bytes >= GB) "${bytes / GB}.${(bytes % GB) * 10 / GB} GB" else "${bytes / MB} MB"

        fun levelLabel(level: String): String = when (level) {
            "critical" -> "Memory is full"
            "warning" -> "Memory is getting full"
            else -> "Healthy"
        }
    }

    /** The numbers for the status tab, in the order memory, CPU, disk. */
    fun lines(): List<Pair<String, String>> = listOf(
        "Memory" to "$ramUsedPercent % used · ${bytesText(ramFree)} free of ${bytesText(ramTotal)}",
        "CPU" to "${Math.round(cpu)} %",
        "Disk" to if (diskTotal > 0) "${bytesText(diskFree)} free of ${bytesText(diskTotal)}" else "Unknown",
    )

    /** The agents, the biggest first. */
    fun agentRows(): List<Pair<String, String>> = agents.sortedByDescending { it.rss }.map { it.botName to bytesText(it.rss) }

    /** One line for a small place; nothing when all is well. */
    fun alert(): String? = when {
        level == "normal" -> null
        refusing -> "Memory $ramUsedPercent % · not starting new turns"
        else -> "Memory $ramUsedPercent % · getting full"
    }
}
