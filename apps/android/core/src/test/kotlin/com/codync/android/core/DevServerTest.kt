package com.codync.android.core

import com.codync.android.core.DevServerMenuRow.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class DevServerTest {
    private fun server(status: DevServerStatus = DevServerStatus.Running, name: String = "web", ports: List<DevServerPort> = emptyList(), managed: Boolean = true) =
        DevServer("a", name, "npm run dev", "/site", status, ports, startedAt = 1, managed = managed)
    private fun port(number: Int, remote: List<String> = emptyList()) = DevServerPort(number, "http://localhost:$number", remote)
    private val tailscale = "http://100.64.0.1:5173"
    private val lan = "http://192.168.1.20:5173"
    private val reachable = port(5173, listOf(lan))

    @Test fun `the title shows nothing without a live server`() {
        assertEquals(DevServerSummary.None, devServerSummary(emptyList()))
        assertEquals(DevServerSummary.None, devServerSummary(listOf(server(DevServerStatus.Stopped))))
    }

    @Test fun `the title shows the port and how many more run`() {
        assertEquals(DevServerSummary.Running(":5173"), devServerSummary(listOf(server(ports = listOf(port(5173))))))
        val three = listOf(server(name = "watch"), server(ports = listOf(port(5173))), server(ports = listOf(port(8787))))
        assertEquals(DevServerSummary.Running(":5173 +2"), devServerSummary(three))
    }

    @Test fun `the title names a server without a port`() {
        assertEquals(DevServerSummary.Running("watch"), devServerSummary(listOf(server(name = "watch"))))
    }

    @Test fun `running outranks starting which outranks failed`() {
        assertEquals(DevServerSummary.Running(":1"), devServerSummary(listOf(server(DevServerStatus.Failed), server(ports = listOf(port(1))))))
        assertEquals(DevServerSummary.Starting, devServerSummary(listOf(server(DevServerStatus.Failed), server(DevServerStatus.Starting))))
        assertEquals(DevServerSummary.Failed, devServerSummary(listOf(server(DevServerStatus.Failed), server(DevServerStatus.Stopped))))
    }

    @Test fun `a row says its ports or its state`() {
        assertEquals(":5173 :24678", server(ports = listOf(port(5173), port(24678))).rowValue)
        assertEquals("Running", server().rowValue)
        assertEquals("Starting", server(DevServerStatus.Starting).rowValue)
        assertEquals("Stopped", server(DevServerStatus.Stopped).rowValue)
        assertEquals("Failed to start", server(DevServerStatus.Failed).rowValue)
    }

    @Test fun `tapping opens what a phone can reach and logs what ended`() {
        val open = DevServerTap.Open("http://192.168.1.20:5173")
        assertEquals(open, server(ports = listOf(reachable)).tap)
        // The first port listens only on the computer; the second doesn't.
        assertEquals(open, server(ports = listOf(port(1), reachable)).tap)
        assertNull(server(ports = listOf(port(5173))).tap)
        assertEquals(DevServerTap.Logs, server().tap)
        assertEquals(DevServerTap.Logs, server(DevServerStatus.Starting).tap)
        assertEquals(DevServerTap.Logs, server(DevServerStatus.Stopped).tap)
        assertEquals(DevServerTap.Logs, server(DevServerStatus.Failed).tap)
        assertNull(server(DevServerStatus.Stopped, managed = false).tap)
        // A stopped server's last address is no longer an address.
        assertEquals(DevServerTap.Logs, server(DevServerStatus.Stopped, ports = listOf(reachable)).tap)
    }

    @Test fun `the menu offers only what applies`() {
        assertEquals(listOf(Open, CopyAddress, Logs, Restart, Stop), server(ports = listOf(reachable)).menuRows)
        assertEquals(listOf(OnlyOnComputer, Logs, Restart, Stop), server(ports = listOf(port(5173))).menuRows)
        assertEquals(listOf(Logs, Restart, Stop), server().menuRows)
        assertEquals(listOf(Logs, Stop), server(DevServerStatus.Starting).menuRows)
        assertEquals(listOf(Logs, Start, Remove), server(DevServerStatus.Stopped).menuRows)
        assertEquals(listOf(Logs, Start, Remove), server(DevServerStatus.Failed).menuRows)
    }

    @Test fun `a tap opens the first address and several are each a row of the menu`() {
        val two = server(ports = listOf(port(5173, listOf(tailscale, lan))))
        assertEquals(DevServerTap.Open(tailscale), two.tap)
        assertEquals(listOf(tailscale, lan), two.remoteAddresses)
        assertEquals(listOf(OpenAddress(tailscale), OpenAddress(lan), CopyAddress, Logs, Restart, Stop), two.menuRows)
        // The first port listens only on the computer; the second doesn't.
        assertEquals(listOf(OpenAddress(tailscale), OpenAddress(lan)), server(ports = listOf(port(1), port(5173, listOf(tailscale, lan)))).menuRows.take(2))
        // Copy address copies the first.
        assertEquals(tailscale, two.remoteAddress)
        assertEquals(listOf(Logs, Start, Remove), server(DevServerStatus.Stopped, ports = listOf(port(5173, listOf(tailscale, lan)))).menuRows)
    }

    @Test fun `a row names an address without its scheme`() {
        assertEquals("192.168.1.20:5173", OpenAddress(lan).title)
    }

    @Test fun `a server left in the bot's shell can only be opened or stopped`() {
        assertEquals(listOf(Open, CopyAddress, StartedByShell, Stop), server(ports = listOf(reachable), managed = false).menuRows)
        assertEquals(listOf(OnlyOnComputer, StartedByShell, Stop), server(ports = listOf(port(5173)), managed = false).menuRows)
    }

    @Test fun `a bot decodes its servers and an older host's without them`() {
        val bot = HostClient.decode(Json.parseToJsonElement("""{"id":"b","name":"Scout","devServers":[{"id":"a","name":"web","command":"npm run dev",
            "folder":null,"status":"running","ports":[{"port":5173,"local":"http://localhost:5173","remote":["http://192.168.1.20:5173"]}],
            "startedAt":null,"exitCode":null,"managed":true},{"id":"c","name":"w","command":"w","status":"sleeping","ports":[],"managed":false}]}""").jsonObject)
        assertEquals("http://192.168.1.20:5173", bot.devServers.first().remoteAddress)
        assertEquals(DevServerStatus.Stopped, bot.devServers.last().status)
        assertTrue(HostClient.decode(Json.parseToJsonElement("""{"id":"b","name":"Scout"}""").jsonObject).devServers.isEmpty())
    }
}
