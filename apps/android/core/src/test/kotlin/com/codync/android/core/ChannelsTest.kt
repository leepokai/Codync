package com.codync.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ChannelsTest {
    private fun parse(text: String) = Json.parseToJsonElement(text)

    @Test fun `the host's status becomes bots without ever carrying a token`() {
        val bots = Channels.decodeTelegram(parse("""{"configured":true,"bots":[{"name":"home","username":"home_bot","botId":"b1","botName":"Agent",
            "allowed":[42],"state":"ok","lastReceivedAt":1000,"pairingExpiresAt":null,"extra":1}],"result":{}}"""))
        assertEquals(listOf(TelegramBot("home", "home_bot", "b1", "Agent", listOf(42L), "ok", 1000L, null)), bots)
    }

    @Test fun `a status with nothing configured is empty`() {
        assertEquals(emptyList<TelegramBot>(), Channels.decodeTelegram(parse("""{"configured":false,"bots":[]}""")))
        assertEquals(SlackStatus(), Channels.decodeSlack(parse("""{"configured":false,"workspaceId":null,"bindings":null,"deliveries":[]}""")))
        assertEquals(SlackStatus(true, true, "T1"), Channels.decodeSlack(parse("""{"configured":true,"socketMode":true,"workspaceId":"T1"}""")))
    }

    @Test fun `a Discord status decodes without a connection or a bound channel`() {
        assertEquals(DiscordStatus(), Channels.decodeDiscord(parse("""{"configured":false,"guildId":null,"botUserId":null,"bindings":null,"state":"starting","deliveries":[]}""")))
        assertEquals(
            DiscordStatus(true, "g1", "u1", listOf(DiscordBinding("c1", "b1", listOf("u2"))), "ok"),
            Channels.decodeDiscord(parse("""{"configured":true,"guildId":"g1","botUserId":"u1","bindings":[{"channelId":"c1","botId":"b1","allowedUsers":["u2"]}],"state":"ok"}""")),
        )
    }

    @Test fun `a pairing answer carries the code once`() {
        assertEquals("ABCD2345", Channels.pairingCode(parse("""{"bots":[],"result":{"code":"ABCD2345","expiresAt":5}}""")))
        assertNull(Channels.pairingCode(parse("""{"bots":[],"result":{}}""")))
    }

    @Test fun `shape rules match the host's`() {
        assertTrue(Channels.nameOk("home-2"))
        assertFalse(Channels.nameOk(""))
        assertFalse(Channels.nameOk("Home"))
        assertFalse(Channels.nameOk("home_2"))
        assertTrue(Channels.tokenOk("123456:" + "a".repeat(35)))
        assertFalse(Channels.tokenOk("123456:short"))
        assertFalse(Channels.tokenOk("abc:" + "a".repeat(35)))
        assertTrue(Channels.idOk("123456789012345678"))
        assertFalse(Channels.idOk("12a4"))
        assertTrue(Channels.discordTokenOk("a".repeat(24) + ".bbb." + "c".repeat(30)))
        assertFalse(Channels.discordTokenOk("nodots" + "c".repeat(30)))
        assertTrue(GitHub.tokenOk("github_pat_" + "A".repeat(30)))
        assertFalse(GitHub.tokenOk("short token"))
    }

    @Test fun `words follow the other clients`() {
        assertEquals("Connected", Channels.stateText("ok"))
        assertEquals("Starting…", Channels.stateText("starting"))
        assertEquals("Cannot reach Telegram", Channels.stateText("error: timed out"))
        val now = 10L * 24 * 3_600_000
        assertEquals("Nothing received yet", Channels.lastReceivedText(null, now))
        assertEquals("Last message just now", Channels.lastReceivedText(now - 20_000, now))
        assertEquals("Last message 5 min ago", Channels.lastReceivedText(now - 5 * 60_000, now))
        assertEquals("Last message 3 h ago", Channels.lastReceivedText(now - 3 * 3_600_000, now))
        assertEquals("Last message 2 d ago", Channels.lastReceivedText(now - 2 * 24 * 3_600_000, now))
        assertEquals("Last message just now", Channels.lastReceivedText(now + 5000, now))
        assertEquals("Send /start ABCD2345 to @home_bot in Telegram. The code works once, for 10 minutes.", Channels.pairingInstruction("ABCD2345", "home_bot"))
        assertEquals("Send /start X to your Telegram bot in Telegram. The code works once, for 10 minutes.", Channels.pairingInstruction("X", ""))
        assertEquals("1 person allowed", Channels.allowedText(1))
        assertEquals("0 people allowed", Channels.allowedText(0))
    }
}
