package com.codync.android

import com.codync.android.core.Computer
import com.codync.android.core.HostError
import com.codync.android.core.HostRoute
import com.codync.android.core.LinkState
import java.io.IOException
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.*
import org.junit.Test

/** A computer whose hello says it could join an account gets the offer, and only that (iOS `JoinAccountTests`). */
class JoinAccountTest {
    @Test fun helloSaysWhetherTheComputerCouldJoin() {
        assertTrue(canJoinAccount(buildJsonObject { put("canJoinAccount", true) }))
        assertFalse(canJoinAccount(buildJsonObject { put("canJoinAccount", false) }))
        assertFalse("an older host doesn't say", canJoinAccount(buildJsonObject { }))
        assertFalse("before the first hello", canJoinAccount(null))
    }

    @Test fun onlyASignedInPhoneReachingTheComputerGetsTheOffer() {
        val computer = Computer("id", "Mac", "key")
        val joinable = buildJsonObject { put("canJoinAccount", true) }
        val ready = AppState(computer, connection = LinkState.Ready(HostRoute.Direct), hello = joinable)
        assertTrue(canAddToAccount(true, ready))
        assertFalse("with no account there's nothing to add it to", canAddToAccount(false, ready))
        assertFalse(canAddToAccount(true, ready.copy(connection = LinkState.Connecting)))
        assertFalse(canAddToAccount(true, ready.copy(hello = buildJsonObject { put("canJoinAccount", false) })))
    }

    @Test fun aFailedAddSaysWhy() {
        assertEquals(AddToAccountFailure.Declined, AddToAccountFailure.of(HostError(500, "The computer stayed out of this account.")))
        assertEquals(AddToAccountFailure.Certificate, AddToAccountFailure.of(HostError(500, "Couldn't confirm your Codync account. Sign in again.")))
        assertEquals(AddToAccountFailure.Certificate, AddToAccountFailure.of(IOException("Connect to the internet to pair, so Codync can confirm your account.")))
        assertEquals(AddToAccountFailure.Unreachable, AddToAccountFailure.of(IOException("Offline")))
        assertEquals(AddToAccountFailure.Other, AddToAccountFailure.of(IllegalStateException("?")))
    }
}
