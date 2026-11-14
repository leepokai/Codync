package com.codync.android.core

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.async
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.*
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ScreenRouteTest {
    @Test fun `transient presence loss waits until a usable route returns`() = runTest {
        var link: LinkState = LinkState.ComputerOffline(null)
        val waiting = async { waitForScreenRoute { link } }
        runCurrent(); advanceTimeBy(1_000); runCurrent()
        assertFalse(waiting.isCompleted)
        link = LinkState.Connecting
        advanceTimeBy(1_000); runCurrent()
        assertFalse(waiting.isCompleted)
        link = LinkState.Ready(HostRoute.Relay)
        advanceTimeBy(100); runCurrent()
        waiting.await()
    }

    @Test fun `access denial and required update fail without retrying`() = runTest {
        for (status in listOf(403, 426)) {
            val result = runCatching { waitForScreenRoute { LinkState.Failed("Unavailable", status) } }
            assertEquals(status, (result.exceptionOrNull() as HostError).status)
        }
    }

    @Test fun `a long outage keeps waiting, as on the iPhone`() = runTest {
        var link: LinkState = LinkState.ComputerOffline(null)
        val waiting = async { waitForScreenRoute { link } }
        runCurrent(); advanceTimeBy(120_000); runCurrent()
        assertFalse(waiting.isCompleted)
        link = LinkState.Ready(HostRoute.Relay)
        advanceTimeBy(100); runCurrent()
        waiting.await()
    }

    @Test fun `closing the screen cancels its route wait`() = runTest {
        val waiting = async { waitForScreenRoute { null } }
        runCurrent(); waiting.cancel(); runCurrent()
        assertTrue(waiting.isCancelled)
    }
}
