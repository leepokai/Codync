package com.codync.android

import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.*
import org.junit.Test

/** A computer whose hello says this phone paired without it joining the account asks to finish (iOS `JoinAccountTests`). */
class JoinAccountTest {
    @Test fun helloAsksToFinishJoiningTheAccount() {
        assertTrue(asksToJoinAccount(buildJsonObject { put("joinAccount", true) }))
        assertFalse(asksToJoinAccount(buildJsonObject { put("joinAccount", false) }))
        assertFalse("an older host doesn't say", asksToJoinAccount(buildJsonObject { }))
    }
}
