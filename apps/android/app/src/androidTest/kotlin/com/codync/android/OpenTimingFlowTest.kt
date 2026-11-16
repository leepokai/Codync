package com.codync.android

import android.app.Application
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.platform.app.InstrumentationRegistry
import com.codync.android.core.*
import java.util.UUID
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

/** A real host and the app's real screens, from a fresh connection through both rendering milestones. */
class OpenTimingFlowTest {
    @get:Rule val compose = createAndroidComposeRule<MainActivity>()

    @Test fun productionClockMeasuresARealDelay() {
        val events = mutableListOf<PhonePerformance.OpenStep>()
        val timing = OpenTiming { events += it }
        timing.beginCold()
        timing.record(OpenPhase.LinkReady, "computer")
        Thread.sleep(20)
        timing.record(OpenPhase.Hello, "computer")

        assertEquals(2, events.size)
        assertTrue(events[1].ms > events[0].ms)
    }

    @Test fun coldAndForegroundOpensRecordAfterTheActualScreensRender() {
        assumeTrue(InstrumentationRegistry.getArguments().getString("liveHost") == "true")
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        requireFixtureHost(LocalStore(context).computers().single().id)
        val store = compose.computerStore()
        compose.waitUntil(30_000) {
            val state = store.state.value
            !state.loading && state.rosterLoaded && state.bots.any { !it.deleted }
        }
        val bot = store.state.value.bots.first { !it.deleted }
        val events = mutableListOf<PhonePerformance.OpenStep>()
        val entriesAtCatchUp = mutableListOf<List<Entry>>()
        val timing = OpenTiming(capture = {
            events += it
            if (it.phase == OpenPhase.CatchUp) entriesAtCatchUp += store.state.value.entries
        })
        compose.runOnUiThread {
            store.setForeground(false)
            store.openTiming = timing
        }
        compose.waitUntil(10_000) { store.channel == null }

        compose.runOnUiThread {
            timing.beginCold()
            store.setForeground(true)
        }
        compose.waitUntil(30_000) { events.map { it.phase }.containsAll(listOf(OpenPhase.CatchUp, OpenPhase.ChatList)) }
        compose.onNodeWithText(bot.name).performClick()
        compose.waitUntil(5_000) { store.state.value.selectedBot == bot.id }
        compose.waitForIdle()
        assertEquals(listOf(OpenPhase.LinkReady, OpenPhase.Hello, OpenPhase.CatchUp, OpenPhase.ChatList),
            events.map { it.phase })
        assertEquals(store.state.value.entries, entriesAtCatchUp.single())

        compose.runOnUiThread {
            // The chat open when the app leaves is the screen its return lands on.
            store.openBot(bot.id)
            timing.enteredBackground()
            store.setForeground(false)
        }
        compose.waitUntil(10_000) { store.channel == null }
        compose.runOnUiThread {
            timing.becameActive()
            store.setForeground(true)
        }
        compose.waitUntil(30_000) { events.size == 8 }
        assertEquals(listOf(OpenPhase.LinkReady, OpenPhase.Hello, OpenPhase.CatchUp, OpenPhase.OpenChat),
            events.drop(4).map { it.phase })

        compose.runOnUiThread { store.openBot(null) }
        compose.waitUntil(30_000) { events.size == 9 }
        assertEquals(OpenPhase.ChatList, events.last().phase)

        assertEquals(listOf(OpenKind.Cold, OpenKind.Foreground), listOf(events.first().open, events[4].open))
        assertEquals(4, events.take(4).map { it.phase }.distinct().size)
        assertEquals(5, events.drop(4).map { it.phase }.distinct().size)
        for (open in listOf(events.take(4), events.drop(4))) {
            assertTrue(open.zipWithNext().all { it.first.ms <= it.second.ms })
            val catchUp = open.single { it.phase == OpenPhase.CatchUp }
            assertTrue((catchUp.chats ?: 0) >= 1)
            assertTrue((catchUp.messages ?: 0) >= 0)
        }
    }

    @Test fun helloRecordsBeforeAHeldRosterAndARetainedLinkReportsNoCatchUpWork() {
        val host = HeldRosterHost()
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val local = LocalStore(context, "open-timing-" + UUID.randomUUID())
        val store = AppStore(context.applicationContext as Application, local, host.computer, "local", 0, kotlinx.coroutines.SupervisorJob(), {}, {})
        val events = mutableListOf<PhonePerformance.OpenStep>()
        val timing = OpenTiming { events += it }
        try {
            runBlocking { store.open() }
            compose.runOnUiThread {
                store.openTiming = timing
                timing.beginCold()
                store.setForeground(true)
            }
            compose.waitUntil(10_000) { host.rosterHeld }
            assertEquals(listOf(OpenPhase.LinkReady, OpenPhase.Hello), events.map { it.phase })

            host.releaseRoster()
            compose.waitUntil(10_000) { events.map { it.phase }.contains(OpenPhase.CatchUp) && store.state.value.bots.size == 2 }
            assertEquals(2, host.eventSubscriptions)
            val subscriptions = host.eventSubscriptions
            compose.runOnUiThread {
                // A call retains transport while the app is backgrounded, so this open reuses the ready link.
                store.mutableState.update { it.copy(voice = VoiceCallViewState("held-call", "bot-1")) }
                timing.enteredBackground()
                store.setForeground(false)
            }
            assertTrue(store.channel?.state?.value is LinkState.Ready)
            compose.runOnUiThread {
                timing.becameActive()
                store.setForeground(true)
            }
            compose.waitUntil(5_000) { events.size == 6 }

            assertEquals(listOf(OpenPhase.LinkReady, OpenPhase.Hello, OpenPhase.CatchUp), events.drop(3).map { it.phase })
            assertEquals(0, events.last().chats)
            assertEquals(0, events.last().messages)
            assertEquals(subscriptions, host.eventSubscriptions)
        } finally {
            compose.runOnUiThread {
                store.mutableState.update { it.copy(voice = null) }
                store.setForeground(false)
            }
            runBlocking { store.retire(unregister = false) }
            local.erase()
            host.close()
        }
    }
}
