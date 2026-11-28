package com.codync.android.core

import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

class BotPauseTest {
    private fun bot(paused: Boolean?) = Bot(id = "b", name = "B", paused = paused)

    @Test fun aHostWithoutTheFieldIsNeverSentItBack() {
        val draft = BotDraft.from(bot(paused = null))
        assertNull("an older host's bot says nothing about pausing", draft.paused)
        assertFalse("an older host is not sent a field it ignores", "paused" in draft.body())
    }

    @Test fun aHostWithTheFieldRoundTripsIt() {
        val paused = BotDraft.from(bot(paused = true))
        assertEquals(true, paused.paused)
        assertEquals("true", paused.body()["paused"]?.jsonPrimitive?.content)

        val resumed = BotDraft.from(bot(paused = false))
        assertEquals(false, resumed.paused)
        assertEquals("false", resumed.body()["paused"]?.jsonPrimitive?.content)
        assertEquals("saving another field keeps the pause it was loaded with", false,
            resumed.copy(name = "Renamed").body()["paused"]?.jsonPrimitive?.content?.toBoolean())
    }
}
