package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.jsonObject

/** The computer's linked WhatsApp number, as `whatsappState` reports it. */
@Serializable data class WhatsAppLink(
    /** `disconnected`, `starting`, `qr`, `connected`, `loggedOut` or `error`. */
    val state: String = "disconnected",
    val qr: String? = null,
    val me: String? = null,
    val name: String? = null,
    val error: String? = null,
    /** The bot whose chat receives this number's messages. */
    val bot: String? = null,
)

/** What the WhatsApp screen shows, and the words it uses; the host owns the meaning. */
object WhatsApp {
    fun decode(response: JsonElement): WhatsAppLink = WireJson.decodeFromJsonElement(WhatsAppLink.serializer(), response.jsonObject)

    fun stateText(state: String): String = when (state) {
        "connected" -> "Linked"
        "starting" -> "Starting…"
        "qr" -> "Scan the code"
        "loggedOut" -> "Not linked"
        "error" -> "The link stopped"
        else -> "Not linked"
    }
}
