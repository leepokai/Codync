package com.codync.android

import com.codync.android.core.Channels
import com.codync.android.core.DiscordStatus
import com.codync.android.core.SlackStatus
import com.codync.android.core.TelegramBot
import kotlinx.serialization.json.JsonElement
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
