package com.codync.android

import com.codync.android.core.*
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.*
import org.junit.Test

class ConnectionPresentationTest {
    private val computer = Computer("fixture", "Studio", "fixture")
    private fun state(link: LinkState, cloud: List<CloudComputer> = emptyList(), computers: List<Computer> = listOf(computer)) = AppState(
        account = AccountState(configured = false), loading = false, setupComplete = true,
        computers = computers, connection = link, actualConnection = link, cloudComputers = cloud)
    private fun cloud(id: String, online: Boolean, name: String = "Studio") = CloudComputer(id, name, "key", online = online)

    @Test fun eachConnectionReadsAsOnIPhone() {
        val revoked = LinkState.Failed("This device isn't allowed on Studio anymore.", 403)
        val cases = listOf(
            LinkState.Ready(HostRoute.Relay) to Triple("Online", "Connected", "Connected through Cloudflare"),
            LinkState.Ready(HostRoute.Direct) to Triple("Online", "Connected", "Connected over Wi-Fi or Tailscale"),
            LinkState.Connecting to Triple("Connecting…", "Connecting…", "Connecting"),
            LinkState.ComputerOffline(null) to Triple("Offline", "Offline", "Offline"),
            LinkState.Failed("Connection closed.") to Triple("Can't reach", "Offline", "Offline"),
            revoked to Triple(revoked.message, "No access", "No access"),
        )
        for ((link, words) in cases) {
            assertEquals(words.first, statusText(state(link)))
            assertEquals(words.second, connectionLabel(state(link)))
            assertEquals(words.third, connectionDescription(state(link)))
        }
        val unpaired = state(LinkState.Connecting, computers = emptyList())
        assertEquals("Not paired", statusText(unpaired))
        assertEquals("Not paired", connectionLabel(unpaired))
        assertEquals("Not paired", connectionDescription(unpaired))
        val custom = computer.copy(cloud = "https://relay.example.com")
        assertEquals("Connected through Private relay (relay.example.com)", connectionDescription(state(LinkState.Ready(HostRoute.Relay), computers = listOf(custom))))
        assertEquals("Connected through Cloudflare", connectionDescription(state(LinkState.Ready(HostRoute.Relay), computers = listOf(computer.copy(cloud = "https://api.codync.dev")))))
        assertEquals("relay.example.com", relayHost("https://relay.example.com/"))
        assertEquals(null, relayHost("https://dev-api.codync.dev"))
        assertEquals("[2001:db8::1]:8443", relayHost("https://[2001:db8::1]:8443"))
        assertEquals("[2001:db8::1]", relayHost("https://[2001:db8::1]"))
        assertEquals("Private relay (relay.example.com)", routeLabel(HostRoute.Relay, "https://relay.example.com"))
        assertEquals("Cloudflare", routeLabel(HostRoute.Relay, "https://api.codync.dev"))
        assertEquals("Wi-Fi/Tailscale", routeLabel(HostRoute.Direct, "https://relay.example.com"))
        assertEquals("Private relay first", routeShortTitle(ConnectionRoute.CloudflareFirst, "https://relay.example.com"))
        assertEquals("Cloudflare first", routeShortTitle(ConnectionRoute.CloudflareFirst, "https://api.codync.dev"))
    }

    @Test fun anOfflineComputerSaysWhenItWasLastSeen() {
        val seen = System.currentTimeMillis() - 60_000
        assertEquals("Offline · seen ${RelativeTime.day(seen)}", statusText(state(LinkState.ComputerOffline(seen))))
    }

    @Test fun aComputerRestartingForAnUpdateAskedHereReadsAsUpdating() {
        val asked = HostUpdate.Requested(System.currentTimeMillis())
        val restarting = state(LinkState.Failed("Connection closed.")).copy(hostUpdate = asked)
        assertTrue(restarting.isUpdating)
        assertEquals("Updating…", statusText(restarting))
        assertFalse(state(LinkState.Ready(HostRoute.Direct)).copy(hostUpdate = asked).isUpdating)
        val longAgo = HostUpdate.Requested(System.currentTimeMillis() - HOST_UPDATE_WAIT_MS - 1)
        assertEquals("Can't reach", statusText(restarting.copy(hostUpdate = longAgo)))
    }

    @Test fun aComputerBehindTheAppNeedsAnUpdate() {
        val online = state(LinkState.Ready(HostRoute.Direct)).copy(hello = buildJsonObject { put("version", "2.0.0"); put("minApp", "2.0.0") })
        assertEquals("Needs update", statusText(online))
        assertEquals("Needs update", connectionLabel(online))
        assertEquals("Needs update", connectionDescription(online))
    }

    @Test fun theHeaderSummarizesTheComputers() {
        assertEquals("1 connected", connectionSummary(state(LinkState.Ready(HostRoute.Direct))))
        assertEquals("Connecting…", connectionSummary(state(LinkState.Connecting)))
        assertEquals("No access", connectionSummary(state(LinkState.Failed("Revoked", 403))))
        assertEquals("1 offline", connectionSummary(state(LinkState.ComputerOffline(null))))
        assertEquals("Computers", connectionSummary(state(LinkState.Connecting, computers = emptyList())))
    }

    @Test fun aComputerTurningThisPhoneAwayOrReplacedByANewIdentityIsStale() {
        assertTrue(state(LinkState.Failed("Revoked", 403)).isStale)
        val offline = state(LinkState.ComputerOffline(null), listOf(cloud("new", online = true)))
        assertTrue(offline.isStale)
        assertEquals("Older copy of this computer · Offline", computerStatusText(offline))
        assertFalse(state(LinkState.ComputerOffline(null), listOf(cloud("new", online = false))).isStale)
        assertFalse(state(LinkState.Ready(HostRoute.Direct), listOf(cloud("new", online = true))).isStale)
        assertEquals("Revoked", computerStatusText(state(LinkState.Failed("Revoked", 403), listOf(cloud("new", online = true)))))
    }

    @Test fun aComputerSeenAfterItsNamesakeJoinedTheAccountRunsBesideItAndIsNotStale() {
        val namesake = cloud("new", online = true).copy(claimedAt = 100)
        fun own(seen: Long) = cloud("fixture", online = false).copy(lastSeenAt = seen)
        // Its own account entry says when it was last seen.
        assertFalse(state(LinkState.ComputerOffline(null), listOf(own(200), namesake)).isStale)
        assertTrue(state(LinkState.ComputerOffline(null), listOf(own(50), namesake)).isStale)
        assertFalse(state(LinkState.Failed("Connection closed."), listOf(own(200), namesake)).isStale)
        // Paired with its code only: the relay's last seen decides.
        assertFalse(state(LinkState.ComputerOffline(200), listOf(namesake)).isStale)
        assertTrue(state(LinkState.ComputerOffline(50), listOf(namesake)).isStale)
        // Without the namesake's join time, the name alone.
        assertTrue(state(LinkState.ComputerOffline(200), listOf(cloud("new", online = true))).isStale)
    }

    @Test fun anOfflineAccountComputerNamedLikeTheCurrentOneIsAnOlderCopy() {
        val old = cloud("old", online = false)
        assertTrue(state(LinkState.Connecting, listOf(old)).isOlderCopy(old))
        assertFalse(state(LinkState.Connecting, listOf(old.copy(online = true))).isOlderCopy(old.copy(online = true)))
        assertFalse(state(LinkState.Connecting, listOf(old), computers = emptyList()).isOlderCopy(old))
        val renamed = cloud("old", online = false, name = "Laptop")
        assertFalse(state(LinkState.Connecting, listOf(renamed)).isOlderCopy(renamed))
        // Not paired here, but the new identity is online in the account.
        val accountOnly = state(LinkState.Connecting, listOf(old, cloud("new", online = true)), computers = emptyList())
        assertTrue(accountOnly.isOlderCopy(old))
        // Seen after the current one joined the account: two machines that share a name.
        val seen = old.copy(lastSeenAt = 200)
        assertFalse(state(LinkState.Connecting, listOf(seen, cloud("fixture", online = true).copy(claimedAt = 100))).isOlderCopy(seen))
        assertTrue(state(LinkState.Connecting, listOf(seen, cloud("fixture", online = true).copy(claimedAt = 300))).isOlderCopy(seen))
    }
}
