package com.codync.android

import com.codync.android.core.Channels
import com.codync.android.core.DiscordBinding
import com.codync.android.core.DiscordStatus
import com.codync.android.core.SlackStatus
import com.codync.android.core.TelegramBot
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/** The messaging channels of the computer: its Telegram bots and Slack connection. Null from an older host. */
suspend fun AppStore.telegramBots(): List<TelegramBot>? = runCatching { Channels.decodeTelegram(hostQuery("telegramStatus", buildJsonObject { })) }.getOrNull()

suspend fun AppStore.slackStatus(): SlackStatus? = runCatching { Channels.decodeSlack(hostQuery("slackStatus", buildJsonObject { })) }.getOrNull()

suspend fun AppStore.disconnectSlack(): SlackStatus = Channels.decodeSlack(hostQuery("disconnectSlack", buildJsonObject { }))

suspend fun AppStore.discordStatus(): DiscordStatus? = runCatching { Channels.decodeDiscord(hostQuery("discordStatus", buildJsonObject { })) }.getOrNull()

suspend fun AppStore.disconnectDiscord(): DiscordStatus = Channels.decodeDiscord(hostQuery("disconnectDiscord", buildJsonObject { }))

/** Sets the Discord bot and its bound channels up; the host checks the token before saving anything. */
suspend fun AppStore.setDiscord(
    guildId: String,
    botUserId: String,
    botToken: String,
    bindings: List<DiscordBinding>,
): DiscordStatus = Channels.decodeDiscord(hostQuery("setDiscordChannel", buildJsonObject {
    put("guildId", guildId.trim())
    put("botUserId", botUserId.trim())
    put("botToken", botToken.trim())
    put("bindings", buildJsonArray {
        for (binding in bindings) add(buildJsonObject {
            put("channelId", binding.channelId.trim())
            put("botId", binding.botId)
            put("allowedUsers", buildJsonArray { for (user in binding.allowedUsers) add(JsonPrimitive(user.trim())) })
        })
    })
}, timeoutMillis = 60_000))

/** Adds a Telegram bot the host will talk through, answered by [botId]; its token stays on the computer. */
suspend fun AppStore.addTelegramBot(name: String, token: String, botId: String): List<TelegramBot> =
    Channels.decodeTelegram(hostQuery("addTelegramBot",
        body("name" to name.trim(), "token" to token.trim(), "botId" to botId), timeoutMillis = 60_000))

suspend fun AppStore.removeTelegramBot(name: String): List<TelegramBot> =
    Channels.decodeTelegram(hostQuery("removeTelegramBot", body("name" to name)))

/** Makes a one-time pairing code for the Telegram bot [name]; the answer also lists the bots. */
suspend fun AppStore.pairTelegram(name: String): JsonElement = hostQuery("pairTelegram", body("name" to name))

/** Lets a Telegram user write to the bot [name], or takes them out. Returns the bots as they are then. */
suspend fun AppStore.setTelegramAllowed(name: String, userId: Long, allowed: Boolean): List<TelegramBot> =
    Channels.decodeTelegram(hostQuery("setTelegramAllowed", buildJsonObject {
        put("name", name)
        put("userId", userId)
        put("allowed", allowed)
    }))
