package com.codync.android

import com.codync.android.core.VersionMismatch
import org.junit.Assert.*
import org.junit.Test

class HostUpdateNoticeTest {
    private val behind = VersionMismatch.UpdateHost("2.23.1", "2.26.0")
    private val needs = "Mini runs Codync 2.23.1; this app needs 2.26.0 or newer."
    private val now = 1_000_000_000L
    private fun notice(canUpdate: Boolean, update: HostUpdate?, os: String? = "macos") = hostUpdateNotice("Mini", behind, os, canUpdate, update, now)

    @Test fun aComputerThatCanUpdateOffersUpdateNow() {
        assertEquals(HostUpdateNotice(needs, true), notice(true, null))
    }

    @Test fun aRequestedUpdateShowsAsUnderWayUntilItsWaitIsOver() {
        assertEquals(HostUpdateNotice("Updating Codync on Mini. This takes a few minutes.", false),
            notice(true, HostUpdate.Requested(now - HOST_UPDATE_WAIT_MS + 1)))
        assertEquals(HostUpdateNotice("$needs Mini didn't finish updating. Try again, or update Codync on that computer.", true),
            notice(true, HostUpdate.Requested(now - HOST_UPDATE_WAIT_MS)))
    }

    @Test fun aRefusedUpdateSaysWhy() {
        val busy = "A bot is working on this computer. Update it once the bot is done."
        assertEquals(HostUpdateNotice("$needs $busy", true), notice(true, HostUpdate.Failed(busy)))
    }

    @Test fun anOlderHostKeepsItsInstructions() {
        assertEquals(HostUpdateNotice("$needs Open Codync on that Mac and choose Check for Updates.", false), notice(false, null))
        assertEquals(HostUpdateNotice("$needs Run codync-host update there.", false), notice(false, null, "linux"))
    }
}
