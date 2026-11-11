package com.codync.android.core

import org.junit.Assert.*
import org.junit.Test

class WakePhraseTest {
    @Test fun only_a_wake_prefix_unblocks_a_turn_and_it_is_not_sent_to_the_bot() {
        val gate = WakePhrase(true)
        assertNull(gate.accept("Please say hey Hermes"))
        assertNull(gate.accept("hey hermesian"))
        assertEquals("Open the report", gate.accept("Hey, Hermes! Open the report"))
        assertNull(gate.accept("unrelated room speech"))
    }
    @Test fun phrase_only_opens_one_short_window_and_lifecycle_reset_rearms_it() {
        var now = 1L
        val gate = WakePhrase(true) { now }
        assertNull(gate.accept("Hey Hermes"))
        assertEquals("the next command", gate.accept("the next command"))
        assertNull(gate.accept("another command"))
        gate.accept("Hey Hermes"); now += 15_001
        assertNull(gate.accept("too late"))
        gate.accept("Hey Hermes"); gate.reset()
        assertNull(gate.accept("after backgrounding"))
        assertEquals("any text", WakePhrase(false).accept("any text"))
    }
}
