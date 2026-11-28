package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonArray
import kotlin.math.ceil

/**
 * An app given MCP access on a computer (`mcpClients`), as a phone shows it: phones list and revoke them, the computer
 * adds them and sets what they may do (iOS `McpApp`).
 */
@Serializable data class McpApp(
    val id: String, val label: String,
    /** `readOnly` | `ask` | `auto` | `full`; an unknown one reads as Read only. */
    val access: String = "readOnly",
    val createdAt: Long = 0, val expiresAt: Long = 0, val revokedAt: Long? = null,
) {
    /** The name of its level, as the computer's settings say it. */
    val levelName: String get() = when (access) {
        "ask" -> "Ask for every request"
        "auto" -> "Auto"
        "full" -> "Full access"
        else -> "Read only"
    }

    /** Whole days until it stops working, 0 once it has. */
    fun daysLeft(nowMillis: Long = System.currentTimeMillis()): Int = maxOf(0, ceil((expiresAt - nowMillis) / 86_400_000.0).toInt())

    companion object {
        /** The apps not revoked, newest first, skipping any it can't read. */
        fun active(response: JsonElement): List<McpApp> =
            ((response as? JsonObject)?.get("clients")?.jsonArray.orEmpty())
                .mapNotNull { runCatching { WireJson.decodeFromJsonElement<McpApp>(it) }.getOrNull() }
                .filter { it.revokedAt == null }.sortedByDescending { it.createdAt }
    }
}
