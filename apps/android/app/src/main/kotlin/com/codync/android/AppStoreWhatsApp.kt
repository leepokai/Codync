package com.codync.android

import com.codync.android.core.WhatsApp
import com.codync.android.core.WhatsAppLink
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** The computer's linked WhatsApp number. Null from an older host. */
suspend fun AppStore.whatsapp(): WhatsAppLink? = runCatching { WhatsApp.decode(hostQuery("whatsappState", buildJsonObject { })) }.getOrNull()

/** Starts the link; the host emits the QR code as an event. */
suspend fun AppStore.whatsappConnect(): JsonElement = hostQuery("whatsappConnect", buildJsonObject { })

/** Forgets the number on this computer. */
suspend fun AppStore.whatsappLogout(): JsonElement = hostQuery("whatsappLogout", buildJsonObject { })

/** Sends the number's messages to [botId], or to none when null. */
suspend fun AppStore.whatsappSetBot(botId: String?): JsonElement = hostQuery("whatsappSetBot", body("botId" to botId))

/** Lets a chat or person write, or takes them out. Returns the list as it is then. */
suspend fun AppStore.whatsappSetAllowed(jid: String, allowed: Boolean): List<String> {
    val answer = hostQuery("whatsappSetAllowed", buildJsonObject { put("jid", jid); put("allowed", allowed) })
    return WhatsApp.decode(answer).allowed
}
