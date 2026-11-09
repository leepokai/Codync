package com.codync.android

import com.codync.android.core.Channels
import com.codync.android.core.DiscordStatus
import com.codync.android.core.SlackStatus
import com.codync.android.core.TelegramBot
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** The messaging channels of the computer: its Telegram bots and Slack connection. Null from an older host. */
suspend fun AppStore.telegramBots(): List<TelegramBot>? = runCatching { Channels.decodeTelegram(hostQuery("telegramStatus", buildJsonObject { })) }.getOrNull()

suspend fun AppStore.slackStatus(): SlackStatus? = runCatching { Channels.decodeSlack(hostQuery("slackStatus", buildJsonObject { })) }.getOrNull()

suspend fun AppStore.discordStatus(): DiscordStatus? = runCatching { Channels.decodeDiscord(hostQuery("discordStatus", buildJsonObject { })) }.getOrNull()

suspend fun AppStore.disconnectDiscord(): DiscordStatus = Channels.decodeDiscord(hostQuery("disconnectDiscord", buildJsonObject { }))

/** Makes a one-time pairing code for the Telegram bot [name]; the answer also lists the bots. */
suspend fun AppStore.pairTelegram(name: String): JsonElement = hostQuery("pairTelegram", body("name" to name))

/** Lets a Telegram user write to the bot [name], or takes them out. Returns the bots as they are then. */
suspend fun AppStore.setTelegramAllowed(name: String, userId: Long, allowed: Boolean): List<TelegramBot> =
    Channels.decodeTelegram(hostQuery("setTelegramAllowed", buildJsonObject {
        put("name", name)
        put("userId", userId)
        put("allowed", allowed)
    }))

/** Saves what the bot should know about a channel (`provider`: telegram, slack or discord). */
suspend fun AppStore.setChannelMemory(provider: String, channel: String, memory: String): JsonElement =
    hostQuery("setChannelMemory", buildJsonObject {
        put("provider", provider)
        put("channel", channel)
        put("memory", memory)
    })


/** Small facts a channel keeps, given to the bot with its memory. */
suspend fun AppStore.channelNotes(provider: String, channel: String): List<String> {
    val answer = hostQuery("channelNotes", buildJsonObject { put("provider", provider); put("channel", channel) }).jsonObject
    return answer["notes"]?.jsonArray?.map { it.jsonPrimitive.content }.orEmpty()
}

suspend fun AppStore.addChannelNote(provider: String, channel: String, text: String): List<String> {
    val answer = hostQuery("addChannelNote", buildJsonObject {
        put("provider", provider); put("channel", channel); put("text", text)
    }).jsonObject
    return answer["notes"]?.jsonArray?.map { it.jsonPrimitive.content }.orEmpty()
}

suspend fun AppStore.removeChannelNote(provider: String, channel: String, index: Int): List<String> {
    val answer = hostQuery("removeChannelNote", buildJsonObject {
        put("provider", provider); put("channel", channel); put("index", index)
    }).jsonObject
    return answer["notes"]?.jsonArray?.map { it.jsonPrimitive.content }.orEmpty()
}

/** Gives a channel a picture (an SF Symbol name), or clears it. */
suspend fun AppStore.setChannelIcon(provider: String, channel: String, icon: String): JsonElement =
    hostQuery("setChannelIcon", buildJsonObject {
        put("provider", provider)
        put("channel", channel)
        put("icon", icon)
    })
