package com.codync.android.core

import org.junit.Assert.*
import org.junit.Test

/** The same table as iOS `ScreenRouteTests`. */
class ScreenTurnTest {
    private fun turn(route: ConnectionRoute, onRelay: Boolean = false, hasTurn: Boolean = true, failed: Int = 0) =
        screenAsksForTurn(onRelay, route, hasTurn, failed)

    @Test fun autoGoesDirectThenThroughTurn() {
        assertFalse(turn(ConnectionRoute.Automatic))
        assertTrue(turn(ConnectionRoute.Automatic, failed = 1))
        assertFalse(turn(ConnectionRoute.Automatic, failed = 2))
        assertTrue("chat came through the relay", turn(ConnectionRoute.Automatic, onRelay = true))
        assertTrue(turn(ConnectionRoute.Automatic, onRelay = true, failed = 1))
    }

    @Test fun cloudflareFirstGoesThroughTurnThenDirect() {
        assertTrue(turn(ConnectionRoute.CloudflareFirst))
        assertFalse(turn(ConnectionRoute.CloudflareFirst, failed = 1))
        assertTrue(turn(ConnectionRoute.CloudflareFirst, onRelay = true))
    }

    @Test fun directOnlyNeverAsks() {
        for (failed in 0..2) assertFalse(turn(ConnectionRoute.DirectOnly, failed = failed))
    }

    @Test fun aComputerInNoAccountHasNoTurn() {
        for (route in ConnectionRoute.entries) for (failed in 0..2) assertFalse(turn(route, hasTurn = false, failed = failed))
    }

    @Test fun aPrivateRelayStillSaysItHasNoScreen() {
        assertTrue(turn(ConnectionRoute.Automatic, onRelay = true, hasTurn = false))
    }
}

class DiscoveredComputerTest {
    @Test fun keepsOnlyValidAddresses() {
        val found = DiscoveredComputer.of("c1", "http://192.168.1.5:19222,not a url,http://10.0.0.2:19222")
        assertEquals(DiscoveredComputer("c1", listOf("http://192.168.1.5:19222", "http://10.0.0.2:19222")), found)
        assertNull(DiscoveredComputer.of(null, "http://192.168.1.5:19222"))
        assertNull(DiscoveredComputer.of("c1", null))
        assertNull(DiscoveredComputer.of("c1", "nope"))
    }
}
