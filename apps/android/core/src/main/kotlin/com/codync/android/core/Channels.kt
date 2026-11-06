package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.jsonObject

/** A Telegram bot of the computer's host, as `telegramStatus` lists it. Never carries the token. */
@Serializable data class TelegramBot(
    val name: String,
    val username: String = "",
    val botId: String = "",
    val botName: String = "",
    val allowed: List<Long> = emptyList(),
    /** `ok`, `starting`, or `error: …`. */
    val state: String = "starting",
    val lastReceivedAt: Long? = null,
    val pairingExpiresAt: Long? = null,
)

/** The Slack connection, as `slackStatus` reports it. */
@Serializable data class SlackStatus(val configured: Boolean = false, val socketMode: Boolean = false, val workspaceId: String? = null)

/** What the Channels screen shows, and the words it uses; the host owns the meaning. */
object Channels {
    fun decodeTelegram(response: JsonElement): List<TelegramBot> =
        WireJson.decodeFromJsonElement(kotlinx.serialization.builtins.ListSerializer(TelegramBot.serializer()), response.jsonObject.getValue("bots"))

    fun decodeSlack(response: JsonElement): SlackStatus = WireJson.decodeFromJsonElement(SlackStatus.serializer(), response)

    /** The pairing code a `pairTelegram` answer carries. */
    fun pairingCode(response: JsonElement): String? =
        (response.jsonObject["result"] as? kotlinx.serialization.json.JsonObject)?.get("code")?.let { (it as? kotlinx.serialization.json.JsonPrimitive)?.content }

    fun stateText(state: String): String = when (state) {
        "ok" -> "Connected"
        "starting" -> "Starting…"
        else -> "Cannot reach Telegram"
    }

    fun lastReceivedText(at: Long?, now: Long): String {
        if (at == null) return "Nothing received yet"
        val minutes = maxOf(0L, (now - at) / 60_000)
        return when {
            minutes < 1 -> "Last message just now"
            minutes < 60 -> "Last message $minutes min ago"
            minutes < 60 * 24 -> "Last message ${minutes / 60} h ago"
            else -> "Last message ${minutes / (60 * 24)} d ago"
        }
    }

    fun pairingInstruction(code: String, username: String): String {
        val where = if (username.isEmpty()) "your Telegram bot" else "@$username"
        return "Send /start $code to $where in Telegram. The code works once, for 10 minutes."
    }

    fun allowedText(count: Int): String = if (count == 1) "1 person allowed" else "$count people allowed"
}
