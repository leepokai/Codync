package com.codync.android

import com.codync.android.core.DiscordStatus
import com.codync.android.core.SlackStatus
import com.codync.android.core.TelegramBot
import com.codync.android.core.WhatsAppLink
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ChannelCatalogTest {
    @Test fun aConnectedTelegramBotCountsAndAFailingOneAsksForAttention() {
        val ok = channelSummaries(listOf(TelegramBot(name = "home", state = "ok")), null, null, null)
        val telegram = ok.first { it.provider == ChannelProvider.Telegram }
        assertTrue(telegram.connected)
        assertFalse(telegram.needsAttention)
        assertEquals("1 bot", telegram.summary)

        val failing = channelSummaries(listOf(TelegramBot(name = "home", state = "error: revoked")), null, null, null)
        assertTrue(failing.first { it.provider == ChannelProvider.Telegram }.needsAttention)
    }

    @Test fun slackAndDiscordReportTheirBoundChannels() {
        val rows = channelSummaries(
            emptyList(),
            SlackStatus(configured = true, socketMode = true, workspaceId = "T1", bindings = emptyList()),
            DiscordStatus(configured = true, guildId = "G", botUserId = "B", state = "ok"),
            null,
        )
        assertTrue(rows.first { it.provider == ChannelProvider.Slack }.connected)
        assertEquals("0 channels", rows.first { it.provider == ChannelProvider.Slack }.summary)
        assertTrue(rows.first { it.provider == ChannelProvider.Discord }.connected)
    }

    @Test fun anUnlinkedNumberIsNotConnectedAndALinkedOneNamesIt() {
        assertFalse(channelSummaries(emptyList(), null, null, null).first().connected)
        val linked = channelSummaries(emptyList(), null, null, WhatsAppLink(state = "connected", me = "+34 600", name = "Work"))
            .first()
        assertTrue(linked.connected)
        assertTrue(linked.summary.contains("Work"))
    }

    @Test fun searchNarrowsByProviderName() {
        val rows = channelSummaries(emptyList(), null, null, null)
        assertEquals(listOf(ChannelProvider.Discord), matchChannelSummaries(rows, "disc").map { it.provider })
        assertEquals(4, matchChannelSummaries(rows, "").size)
    }
}
