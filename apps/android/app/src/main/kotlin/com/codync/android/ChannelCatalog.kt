package com.codync.android

import com.codync.android.core.Channels
import com.codync.android.core.DiscordStatus
import com.codync.android.core.SlackStatus
import com.codync.android.core.TelegramBot
import com.codync.android.core.WhatsAppLink

/** The messaging channels a computer can host, as Extensions lists them. */
internal enum class ChannelProvider(val title: String, val icon: String, val description: String) {
    WhatsApp("WhatsApp", "bubble.left.and.bubble.right", "Link a WhatsApp number so your bots read and answer its chats. A separate number is safer."),
    Telegram("Telegram", "paperplane", "Talk to a bot from Telegram. Only people you pair can write."),
    Slack("Slack", "number", "Answers in the Slack channels you bind. Keys stay in the computer."),
    Discord("Discord", "gamecontroller", "Answers in your server's channels, approvals included."),
}

/** One row in Extensions: whether it is connected and the line under its name. */
internal data class ChannelSummary(
    val provider: ChannelProvider,
    val connected: Boolean,
    val needsAttention: Boolean,
    val summary: String,
)

internal fun stateOf(raw: String): String = Channels.stateText(raw)

/** The four channels, from what the host already reports (same rules as the desktop catalog). */
internal fun channelSummaries(
    bots: List<TelegramBot>,
    slack: SlackStatus?,
    discord: DiscordStatus?,
    whatsapp: WhatsAppLink?,
): List<ChannelSummary> {
    val whatsappConnected = whatsapp?.state == "connected"
    val telegramConnected = bots.isNotEmpty()
    val slackConnected = slack?.configured == true
    val discordConnected = discord?.configured == true
    return listOf(
        ChannelSummary(
            ChannelProvider.WhatsApp,
            whatsappConnected,
            whatsapp?.state in setOf("error", "loggedOut"),
            if (!whatsappConnected) "Not connected"
            else listOfNotNull(whatsapp?.name ?: whatsapp?.me, stateOf(whatsapp?.state.orEmpty())).joinToString(" · "),
        ),
        ChannelSummary(
            ChannelProvider.Telegram,
            telegramConnected,
            bots.any { it.state.startsWith("error") },
            if (!telegramConnected) "Not connected" else "${bots.size} ${if (bots.size == 1) "bot" else "bots"}",
        ),
        ChannelSummary(
            ChannelProvider.Slack,
            slackConnected,
            false,
            if (!slackConnected) "Not connected" else "${slack?.bindings?.size ?: 0} channels",
        ),
        ChannelSummary(
            ChannelProvider.Discord,
            discordConnected,
            discordConnected && discord?.state?.startsWith("error") == true,
            if (!discordConnected) "Not connected" else "${discord?.bindings?.size ?: 0} channels",
        ),
    )
}

/** The channels matching a search by provider name or summary. */
internal fun matchChannelSummaries(rows: List<ChannelSummary>, query: String): List<ChannelSummary> {
    val q = query.trim()
    if (q.isEmpty()) return rows
    return rows.filter { "${it.provider.title} ${it.provider.description} ${it.summary}".contains(q, true) }
}
