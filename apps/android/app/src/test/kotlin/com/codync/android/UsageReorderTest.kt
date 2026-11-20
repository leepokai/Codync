package com.codync.android

import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class UsageReorderTest {
    private val row = 40
    private val saved = mutableListOf<List<String>>()

    private fun reorder(vararg ids: String) = ListReorder().apply {
        names = ids.toList()
        ids.forEach { heights[it] = row }
        position = { shown.indexOf(it) * row.toFloat() }
        save = { saved.add(it) }
    }

    @Test fun aDroppedRowLandsWhereItWasLetGo() {
        val drag = reorder("claude", "codex", "cursor")
        drag.handle("claude").apply { start(); move(30f); end() }
        assertEquals(listOf(listOf("codex", "claude", "cursor")), saved)
        saved.clear()
        val back = reorder("claude", "codex", "cursor")
        back.handle("cursor").apply { start(); move(-30f); move(-40f); end() }
        assertEquals(listOf(listOf("cursor", "claude", "codex")), saved)
    }

    @Test fun aRowDroppedInItsOwnPlaceSendsNothing() {
        val drag = reorder("claude", "codex")
        drag.handle("claude").apply { start(); move(10f); end() }
        assertEquals(emptyList<List<String>>(), saved)
    }

    @Test fun anAccessibilityStepMovesOnePlaceAndStopsAtTheEnds() {
        val drag = reorder("claude", "codex", "cursor")
        drag.move("codex", -1)
        drag.move("codex", 1)
        drag.move("claude", -1)
        drag.move("cursor", 1)
        assertEquals(listOf(listOf("codex", "claude", "cursor"), listOf("claude", "cursor", "codex")), saved)
    }

    @Test fun aDragThatSwapsAsksForTheListToHoldItsPlaceFirst() {
        var pinned = 0
        val drag = reorder("claude", "codex", "cursor").apply { beforeSwap = { pinned++ } }
        drag.move("claude", 1)
        assertEquals("an accessibility step isn't a drag", 0, pinned)
        drag.handle("cursor").apply { start(); move(-10f); assertEquals(0, pinned); move(-20f); assertEquals(1, pinned) }
    }

    @Test fun aHeldRowLeavingTheListEndsTheDragAndAnotherLeavingDoesNot() {
        val drag = reorder("claude", "codex", "cursor")
        drag.handle("claude").apply { start(); move(30f) }
        drag.released("cursor")
        assertEquals("claude", drag.dragged)
        drag.released("claude")
        assertNull(drag.dragged)
        assertEquals(listOf("claude", "codex", "cursor"), drag.shown)
        assertEquals(emptyList<List<String>>(), saved)
    }

    @Test fun dropsWhileOneIsOnItsWayLeaveOnlyTheLastToSend() = runBlocking {
        val waiting = Channel<String>(Channel.CONFLATED)
        val sent = mutableListOf<String>()
        waiting.trySend("first")
        waiting.sendEach { order ->
            sent += order
            if (order == "first") { waiting.trySend("second"); waiting.trySend("third") }
        }
        assertEquals(listOf("first", "third"), sent)
    }
}
