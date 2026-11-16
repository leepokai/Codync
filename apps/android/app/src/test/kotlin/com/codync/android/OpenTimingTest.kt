package com.codync.android

import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds
import org.junit.Assert.assertEquals
import org.junit.Test

/** `open_timing`'s clock and once-per-open bookkeeping (iOS `OpenTimingTests`). */
class OpenTimingTest {
    /** Each call moves the clock one millisecond, as the iOS tests' does. */
    private fun ticking(): Pair<() -> Duration, MutableList<PhonePerformance.OpenStep>> {
        var clock = Duration.ZERO
        val events = mutableListOf<PhonePerformance.OpenStep>()
        return { clock += 1.milliseconds; clock } to events
    }

    @Test fun `one open records each timing once in order`() {
        val (now, events) = ticking()
        val timing = OpenTiming(now) { events += it }
        timing.beginCold()
        timing.becameActive()
        timing.record(OpenPhase.LinkReady, "computer")
        timing.record(OpenPhase.Hello, "computer")
        timing.record(OpenPhase.CatchUp, "computer", chats = 1, messages = 2)
        timing.record(OpenPhase.ChatList, "computer")
        timing.record(OpenPhase.OpenChat, "computer")
        // Already sent: every phase reports once per computer and open.
        timing.record(OpenPhase.LinkReady, "computer")
        timing.record(OpenPhase.Hello, "computer")
        timing.record(OpenPhase.CatchUp, "computer", chats = 1, messages = 2)
        timing.record(OpenPhase.ChatList, "computer")
        timing.record(OpenPhase.OpenChat, "computer")

        val sent = events
        assertEquals(listOf(OpenPhase.LinkReady, OpenPhase.Hello, OpenPhase.CatchUp, OpenPhase.ChatList, OpenPhase.OpenChat),
            sent.map { it.phase })
        assertEquals(listOf(1L, 2L, 3L, 4L, 5L), sent.map { it.ms })
        for (event in sent) {
            assertEquals(OpenKind.Cold, event.open)
        }
        assertEquals(1, sent[2].chats)
        assertEquals(2, sent[2].messages)
    }

    @Test fun `each computer reports its own milestones`() {
        val (now, events) = ticking()
        val timing = OpenTiming(now) { events += it }
        timing.beginCold()
        timing.record(OpenPhase.LinkReady, "one")
        timing.record(OpenPhase.LinkReady, "two")
        timing.record(OpenPhase.LinkReady, "one")
        assertEquals(2, events.size)
    }

    @Test fun `returning from the background starts a new timing open`() {
        val (now, events) = ticking()
        val timing = OpenTiming(now) { events += it }
        timing.beginCold()
        timing.becameActive()
        timing.record(OpenPhase.LinkReady, "computer")
        // Not backgrounded first: the same open goes on, its milestones still sent.
        timing.becameActive()
        assertEquals(1, events.size)
        timing.enteredBackground()
        timing.becameActive()
        timing.record(OpenPhase.LinkReady, "computer")

        val sent = events
        assertEquals(2, sent.size)
        assertEquals(OpenKind.Cold, sent[0].open)
        assertEquals(OpenKind.Foreground, sent[1].open)
        assertEquals(1L, sent[0].ms)
        assertEquals(1L, sent[1].ms)
        assertEquals(2, timing.opens)
    }

    @Test fun `nothing records before an open begins`() {
        val (now, events) = ticking()
        val timing = OpenTiming(now) { events += it }
        timing.record(OpenPhase.LinkReady, "computer")
        assertEquals(0, events.size)
    }
}
