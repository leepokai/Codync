package com.codync.android.core

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class CompactionTest {
    private fun bot(fields: String): Bot = WireJson.decodeFromString("""{"id": "b", "name": "B" $fields}""")

    @Test fun `a bot reads each compaction choice, and a newer host's name as early`() {
        assertEquals(Compaction.Early, bot(""", "compaction": "early"""").compaction)
        assertEquals(Compaction.NearLimit, bot(""", "compaction": "nearLimit"""").compaction)
        assertEquals(Compaction.Off, bot(""", "compaction": "off"""").compaction)
        assertEquals(Compaction.Tokens(120_000), bot(""", "compaction": 120000""").compaction)
        assertEquals(Compaction.Early, bot(""", "compaction": "later"""").compaction)
        assertNull("no setting where Codync doesn't summarize", bot(""", "compaction": null""").compaction)
        assertNull(bot("").compaction)
    }

    @Test fun `the sizes run from min to max by step`() {
        val read = bot(""", "compaction": "early", "compactionTokens": {"min": 50000, "max": 960000, "step": 10000}""")
        val sizes = read.compactionTokens!!.sizes
        assertEquals(92, sizes.size)
        assertEquals(listOf(50_000L, 60_000L), sizes.take(2))
        assertEquals(960_000L, sizes.last())
        assertEquals("120K", Compaction.Tokens(120_000).label)
        assertNull(bot(""", "compactionTokens": null""").compactionTokens)
    }

    @Test fun `a draft sends the choice as the host reads it, and nothing where there is none`() {
        assertEquals(JsonPrimitive("nearLimit"), BotDraft.from(bot(""", "compaction": "nearLimit"""")).body()["compaction"])
        assertEquals(JsonPrimitive(300_000), BotDraft.from(bot("")).copy(compaction = Compaction.Tokens(300_000)).body()["compaction"])
        assertFalse("compaction" in BotDraft.from(bot("")).body())
        // A stored bot reads back as written (the mirror database keeps bots as JSON).
        val stored = bot(""", "compaction": 80000""")
        assertEquals(stored, WireJson.decodeFromString<Bot>(WireJson.encodeToString(Bot.serializer(), stored)))
    }
}
