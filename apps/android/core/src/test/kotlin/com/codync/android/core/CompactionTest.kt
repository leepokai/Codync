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

    @Test fun `the menu lists the presets, and a typed size fits the window`() {
        val read = bot(""", "compaction": "early", "compactionTokens": {"min": 50000, "max": 960000, "step": 10000, "presets": [100000, 200000]}""")
        val tokens = read.compactionTokens!!
        assertEquals(listOf(100_000L, 200_000L), tokens.presets)
        assertEquals(listOf(350_000L, 360_000L, 50_000L, 960_000L), listOf(354_000L, 355_000L, 10_000L, 5_000_000L).map(tokens::fit))
        assertEquals("120K", Compaction.Tokens(120_000).label)
        assertNull(bot(""", "compactionTokens": null""").compactionTokens)
        val small = bot(""", "compactionTokens": {"min": 50000, "max": 90000, "step": 10000, "presets": []}""")
        assertEquals(emptyList<Long>(), small.compactionTokens!!.presets)
    }

    @Test fun `a typed size reads in thousands of tokens`() {
        assertEquals(350_000L, Compaction.typed("350"))
        assertEquals(350_000L, Compaction.typed("350k"))
        assertEquals(1_500L, Compaction.typed(" 1.5K "))
        assertNull(Compaction.typed("abc"))
        assertNull(Compaction.typed(""))
        assertNull(Compaction.typed("0"))
        assertNull(Compaction.typed("99999999999999999"))
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
