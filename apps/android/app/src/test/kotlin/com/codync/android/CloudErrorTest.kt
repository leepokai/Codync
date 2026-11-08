package com.codync.android

import org.junit.Assert.*
import org.junit.Test

/** Cloud providers shows a key's error under that key, anything else below every provider (iOS `cloudKeyComesBeforeTheComputer`). */
class CloudErrorTest {
    @Test fun keyErrorShowsUnderItsOwnKey() {
        val refused = AccountContext(error = "This key wasn't accepted.", failedAction = cloudKeyAction("boat"))
        assertEquals("This key wasn't accepted.", refused.cloudErrorFor("boat"))
        assertNull(refused.cloudErrorFor("instabox"))
        assertNull(refused.cloudErrorFor(null))

        val other = AccountContext(error = "Your cloud computer is asleep.", failedAction = "cloudMachine")
        assertNull(other.cloudErrorFor("boat"))
        assertEquals("Your cloud computer is asleep.", other.cloudErrorFor(null))
    }
}
