package com.codync.android

import org.junit.Assert.assertEquals
import org.junit.Test

class UndecodableTest {
    @Test fun `an unreadable event fetches everything again once, then waits for an update or passes it over`() {
        assertEquals(Undecodable.Rewind, undecodable(alreadyRewound = false, hostNewer = true))
        assertEquals(Undecodable.Rewind, undecodable(alreadyRewound = false, hostNewer = false))
        assertEquals(Undecodable.AwaitUpdate, undecodable(alreadyRewound = true, hostNewer = true))
        assertEquals(Undecodable.Skip, undecodable(alreadyRewound = true, hostNewer = false))
    }
}
