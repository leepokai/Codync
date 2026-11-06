package com.codync.android.core

import java.io.IOException
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class BotSnapshotTest {
    private fun header(rev: Long) = buildJsonObject { put("type", "hello"); put("rev", rev) }
    private fun bot(id: String, rev: Long, deleted: Boolean = false) = buildJsonObject {
        put("type", "bot"); put("rev", rev)
        put("bot", buildJsonObject { put("id", id); put("name", id); put("rev", rev); put("deleted", deleted) })
    }

    @Test fun `a roster waits for catch-up and cancels its subscription at the advertised head`() = runBlocking {
        var cancelled = false
        val stream = flow {
            try {
                emit(header(8)); emit(bot("first", 2)); emit(bot("removed", 3)); emit(bot("removed", 4, true))
                emit(buildJsonObject { put("type", "usage"); put("rev", 8) })
                emit(bot("last", 7))
                emit(buildJsonObject { put("type", "entry"); put("rev", 8) })
                fail("The finite snapshot must close its subscription before later live events")
            } finally { cancelled = true }
        }
        val bots = botSnapshot(stream)
        assertEquals(listOf("first", "last"), bots.map { it.id })
        assertTrue(cancelled)
    }

    @Test fun `a roster ends at the last rev the catch-up sends, not at revs it never replays`() = runBlocking {
        // rev 11 went to memory and a paired device; the catch-up's last event is the bot at 8.
        val hello = buildJsonObject { put("type", "hello"); put("rev", 11); put("catchUpRev", 8) }
        val stream = flow { emit(hello); emit(bot("first", 2)); emit(bot("last", 8)); kotlinx.coroutines.awaitCancellation() }
        assertEquals(listOf("first", "last"), botSnapshot(stream).map(Bot::id))
    }

    @Test fun `a new empty host finishes without waiting for another event`() = runBlocking {
        assertTrue(botSnapshot(flowOf(header(0))).isEmpty())
    }

    @Test fun `an incomplete or resyncing snapshot is an error rather than an empty roster`() {
        assertThrows(IllegalStateException::class.java) { runBlocking { botSnapshot(flowOf(header(8), bot("partial", 2))) } }
        assertThrows(IOException::class.java) { runBlocking {
            botSnapshot(flowOf(header(8), buildJsonObject { put("type", "resync") }))
        } }
    }
}
