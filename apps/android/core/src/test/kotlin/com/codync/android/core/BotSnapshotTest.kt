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

    @Test fun `a new empty host finishes without waiting for another event`() = runBlocking {
        assertTrue(botSnapshot(flowOf(header(0))).isEmpty())
    }

    @Test fun `an incomplete or resyncing snapshot is an error rather than an empty roster`() {
        assertThrows(IllegalStateException::class.java) { runBlocking { botSnapshot(flowOf(header(8), bot("partial", 2))) } }
        assertThrows(IOException::class.java) { runBlocking {
            botSnapshot(flowOf(header(8), buildJsonObject { put("type", "resync") }))
        } }
    }

    @Test fun `icon-only revisions complete the advertised catch-up and decorate bot faces`() = runBlocking {
        val icon = CustomIcon("data:image/png;base64,iVBORw0KGgo=", true)
        val stream = flow {
            emit(buildJsonObject { put("type", "hello"); put("rev", 50); put("catchUp", 2) })
            emit(buildJsonObject { put("type", "icons"); put("iconsRevision", 50)
                put("icons", WireJson.encodeToJsonElement(mapOf("bot:b" to icon))) })
            emit(bot("b", 3))
            fail("An icon revision need not be a bot/entry revision to finish the snapshot")
        }
        assertEquals(icon, botSnapshot(stream).single().customIcon)
    }

    @Test fun `empty icon snapshot removes stale inline custom faces`() = runBlocking {
        val result = botSnapshot(flowOf(
            buildJsonObject { put("type", "hello"); put("rev", 3); put("catchUp", 2) },
            buildJsonObject { put("type", "icons"); put("icons", buildJsonObject {}) },
            buildJsonObject { put("type", "bot"); put("rev", 3)
                put("bot", WireJson.encodeToJsonElement(Bot("b", "Bot", rev = 3, customIcon = CustomIcon("stale")))) },
        ))
        assertNull(result.single().customIcon)
    }
}
