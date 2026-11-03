package com.codync.android

import com.codync.android.core.Computer
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.*
import org.junit.Test

/** A computer whose hello says this phone paired without it joining the account asks to finish (iOS `JoinAccountTests`). */
class JoinAccountTest {
    private val computer = Computer("id", "Mac", "key")

    @Test fun helloAsksToFinishJoiningTheAccount() {
        assertTrue(asksToJoinAccount(buildJsonObject { put("joinAccount", true) }, computer))
        assertFalse(asksToJoinAccount(buildJsonObject { put("joinAccount", false) }, computer))
        assertFalse("an older host doesn't say", asksToJoinAccount(buildJsonObject { }, computer))
    }

    @Test fun aComputerThisPhoneBroughtInIsNotBroughtBackAfterItWasTakenOut() {
        val joined = computer.copy(joinedAccount = true)
        assertFalse(asksToJoinAccount(buildJsonObject { put("joinAccount", true) }, joined))
        assertTrue("a newer copy from the computer's hello keeps it", computer.keepingDevicePicks(joined).joinedAccount)
    }
}
