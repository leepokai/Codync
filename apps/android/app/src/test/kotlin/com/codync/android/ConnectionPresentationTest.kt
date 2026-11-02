package com.codync.android

import com.codync.android.core.*
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.*
import org.junit.Test

class ConnectionPresentationTest {
    private val computer = Computer("fixture", "Studio", "fixture")
    private fun state(link: LinkState, computer: Computer = this.computer) = AppState(computer, loading = false, connection = link, actualConnection = link)
    private fun context(cloud: List<CloudComputer> = emptyList(), computers: List<Computer> = listOf(computer)) = AccountContext(
        account = AccountState(configured = false), loading = false, setupComplete = true, computers = computers, cloudComputers = cloud)
    private fun cloud(id: String, online: Boolean, name: String = "Studio") = CloudComputer(id, name, "key", online = online)
    private fun stale(link: LinkState, cloud: List<CloudComputer> = emptyList(), others: List<AppState> = emptyList()) =
        context(cloud).isStale(state(link), others)

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
        val custom = computer.copy(cloud = "https://relay.example.com")
        assertEquals("Connected through Private relay (relay.example.com)", connectionDescription(state(LinkState.Ready(HostRoute.Relay), custom)))
        assertEquals("Connected through Cloudflare", connectionDescription(state(LinkState.Ready(HostRoute.Relay), computer.copy(cloud = "https://api.codync.dev"))))
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
        assertEquals("1 connected", connectionSummary(listOf(state(LinkState.Ready(HostRoute.Direct)))).filled())
        assertEquals("Connecting…", connectionSummary(listOf(state(LinkState.Connecting))).filled())
        assertEquals("No access", connectionSummary(listOf(state(LinkState.Failed("Revoked", 403)))).filled())
        assertEquals("1 offline", connectionSummary(listOf(state(LinkState.ComputerOffline(null)))).filled())
        assertEquals("Computers", connectionSummary(emptyList()).filled())
        val laptop = Computer("laptop", "Laptop", "laptop")
        assertEquals("2 connected", connectionSummary(listOf(state(LinkState.Ready(HostRoute.Direct)), state(LinkState.Ready(HostRoute.Relay), laptop))).filled())
        assertEquals("1/2 connected", connectionSummary(listOf(state(LinkState.Ready(HostRoute.Direct)), state(LinkState.ComputerOffline(null), laptop))).filled())
        assertEquals("2 offline", connectionSummary(listOf(state(LinkState.Failed("Revoked", 403)), state(LinkState.ComputerOffline(null), laptop))).filled())
    }

    @Test fun aComputerTurningThisPhoneAwayOrReplacedByANewIdentityIsStale() {
        assertTrue(stale(LinkState.Failed("Revoked", 403)))
        val offline = state(LinkState.ComputerOffline(null))
        assertTrue(stale(LinkState.ComputerOffline(null), listOf(cloud("new", online = true))))
        assertEquals("Older copy of this computer · Offline", computerStatusText(offline, stale = true))
        assertEquals("Offline", computerStatusText(offline, stale = false))
        assertFalse(stale(LinkState.ComputerOffline(null), listOf(cloud("new", online = false))))
        assertFalse(stale(LinkState.Ready(HostRoute.Direct), listOf(cloud("new", online = true))))
        val revoked = LinkState.Failed("Revoked", 403)
        assertEquals("Revoked", computerStatusText(state(revoked), stale(revoked, listOf(cloud("new", online = true)))))
    }

    @Test fun anOfflineComputerIsStaleWhenAnotherSavedComputerWithItsNameIsOnlineAndJoinedLater() {
        val namesake = Computer("new", "Studio", "new")
        val joined = listOf(cloud("new", online = false).copy(claimedAt = 100))
        // Paired here with its code only: the name decides.
        assertTrue(stale(LinkState.ComputerOffline(null), others = listOf(state(LinkState.Ready(HostRoute.Direct), namesake))))
        assertFalse(stale(LinkState.ComputerOffline(null), others = listOf(state(LinkState.ComputerOffline(null), namesake))))
        assertFalse(stale(LinkState.ComputerOffline(null), others = listOf(state(LinkState.Ready(HostRoute.Direct), namesake.copy(name = "Laptop")))))
        // It was last seen before the namesake joined the account: replaced; after: two machines.
        assertTrue(stale(LinkState.ComputerOffline(50), joined, listOf(state(LinkState.Ready(HostRoute.Direct), namesake))))
        assertFalse(stale(LinkState.ComputerOffline(200), joined, listOf(state(LinkState.Ready(HostRoute.Direct), namesake))))
        // Its own state among the others never makes it stale.
        assertFalse(stale(LinkState.ComputerOffline(null), others = listOf(state(LinkState.Ready(HostRoute.Direct)))))
    }

    @Test fun aComputerSeenAfterItsNamesakeJoinedTheAccountRunsBesideItAndIsNotStale() {
        val namesake = cloud("new", online = true).copy(claimedAt = 100)
        fun own(seen: Long) = cloud("fixture", online = false).copy(lastSeenAt = seen)
        // Its own account entry says when it was last seen.
        assertFalse(stale(LinkState.ComputerOffline(null), listOf(own(200), namesake)))
        assertTrue(stale(LinkState.ComputerOffline(null), listOf(own(50), namesake)))
        assertFalse(stale(LinkState.Failed("Connection closed."), listOf(own(200), namesake)))
        // Paired with its code only: the relay's last seen decides.
        assertFalse(stale(LinkState.ComputerOffline(200), listOf(namesake)))
        assertTrue(stale(LinkState.ComputerOffline(50), listOf(namesake)))
        // Without the namesake's join time, the name alone.
        assertTrue(stale(LinkState.ComputerOffline(200), listOf(cloud("new", online = true))))
    }

    @Test fun anOfflineAccountComputerNamedLikeTheCurrentOneIsAnOlderCopy() {
        val old = cloud("old", online = false)
        assertTrue(context(listOf(old)).isOlderCopy(old))
        assertFalse(context(listOf(old.copy(online = true))).isOlderCopy(old.copy(online = true)))
        assertFalse(context(listOf(old), computers = emptyList()).isOlderCopy(old))
        val renamed = cloud("old", online = false, name = "Laptop")
        assertFalse(context(listOf(renamed)).isOlderCopy(renamed))
        // Not paired here, but the new identity is online in the account.
        assertTrue(context(listOf(old, cloud("new", online = true)), computers = emptyList()).isOlderCopy(old))
        // Seen after the current one joined the account: two machines that share a name.
        val seen = old.copy(lastSeenAt = 200)
        assertFalse(context(listOf(seen, cloud("fixture", online = true).copy(claimedAt = 100))).isOlderCopy(seen))
        assertTrue(context(listOf(seen, cloud("fixture", online = true).copy(claimedAt = 300))).isOlderCopy(seen))
        // Two computers paired here: the older copy of the second one is found too.
        val laptop = Computer("laptop", "Laptop", "laptop")
        assertTrue(context(listOf(renamed), computers = listOf(computer, laptop)).isOlderCopy(renamed))
    }
}
