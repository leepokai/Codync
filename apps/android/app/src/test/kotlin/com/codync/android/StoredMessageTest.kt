package com.codync.android

import com.codync.android.core.Entry
import com.codync.android.core.WireJson
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.putJsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class StoredMessageTest {
    @Test fun `a message that fits is one piece`() {
        assertEquals(listOf(0 until 5), "hello".storedChunkRanges(10))
    }

    @Test fun `a long message is cut into pieces of its size and loses nothing`() {
        val text = (1..100).joinToString("\n") { "line $it" }
        val ranges = text.storedChunkRanges(40)
        assertEquals(text, ranges.joinToString("") { text.substring(it) })
        assertTrue(ranges.size > 1)
        assertTrue(ranges.all { it.last - it.first + 1 <= 40 })
    }

    @Test fun `a piece ends at a line break when one comes in its second half`() {
        val text = "a".repeat(30) + "\n" + "b".repeat(60)
        val ranges = text.storedChunkRanges(40)
        assertEquals("a".repeat(30) + "\n", text.substring(ranges.first()))
        assertEquals(text, ranges.joinToString("") { text.substring(it) })
    }

    @Test fun `a long line is cut at its size`() {
        val text = "c".repeat(95)
        val ranges = text.storedChunkRanges(40)
        assertEquals(listOf(40, 40, 15), ranges.map { it.last - it.first + 1 })
        assertEquals(text, ranges.joinToString("") { text.substring(it) })
    }

    @Test fun `the pieces cover the message once with no empty one at the end`() {
        val text = "d".repeat(80)
        val ranges = text.storedChunkRanges(40)
        assertEquals(listOf(0 until 40, 40 until 80), ranges)
    }

    @Test fun `a line break at either edge of the search half keeps exact pieces`() {
        fun textWithBreak(at: Int) = "x".repeat(80).toCharArray().also { it[at] = '\n' }.concatToString()
        for ((at, firstEnd) in listOf(19 to 40, 20 to 21, 39 to 40)) {
            val text = textWithBreak(at)
            val ranges = text.storedChunkRanges(40)
            assertEquals("break at $at", firstEnd, ranges.first().last - ranges.first().first + 1)
            assertEquals("break at $at", text, ranges.joinToString("") { text.substring(it) })
        }
    }

    @Test(timeout = 5_000) fun `a giant single line is cut once in linear time`() {
        val text = "e".repeat(2_000_000)
        val ranges = text.storedChunkRanges()
        assertEquals(500, ranges.size)
        assertTrue(ranges.all { it.last - it.first + 1 == STORED_CHUNK_CHARS })
        assertEquals(0, ranges.first().first)
        assertEquals(text.length - 1, ranges.last().last)
    }

    @Test fun `a payload the read cut off opens with the part that arrived and says it was cut`() {
        val text = "M".repeat(PER_MESSAGE_CHARS * 2)
        val stored = Entry("e-cut", 1, "a", rev = 1, kind = "agent", data = buildJsonObject {
            put("text", text)
            put("final", true)
        })
        val payload = WireJson.encodeToString(stored)
        val shown = storedTextOfPayload(payload.take(PER_MESSAGE_CHARS), "e-cut", complete = false)
        assertTrue(shown.startsWith("MMMM"))
        assertEquals("the formatter marks a message longer than the sheet reads", STORED_CUT_MARK, shown.takeLast(1))
        assertTrue("the slice's whole body is kept, not a field or two", shown.length > PER_MESSAGE_CHARS - 200)
        // The row is unchanged: the same bound reads the same front.
        assertEquals(shown, storedTextOfPayload(payload.take(PER_MESSAGE_CHARS), "e-cut", complete = false))
    }

    @Test fun `a cut tool payload keeps the patches it holds`() {
        val patch = "+ " + "line\n+ ".repeat(PER_MESSAGE_CHARS)
        val stored = Entry("e-tool", 2, "a", rev = 2, kind = "tool", data = buildJsonObject {
            put("toolKind", "edit")
            put("title", "Edit src/a.kt")
            put("diffs", buildJsonArray { add(buildJsonObject { put("path", "src/a.kt"); put("patch", patch) }) })
        })
        val payload = WireJson.encodeToString(stored)
        assertTrue(payload.length > PER_MESSAGE_CHARS)
        val shown = storedTextOfPayload(payload.take(PER_MESSAGE_CHARS), "e-tool", complete = false)
        assertTrue(shown.startsWith("src/a.kt\n+ "))
        assertEquals(STORED_CUT_MARK, shown.takeLast(1))
    }

    @Test fun `a cut inside a unicode escape still opens with the text before it`() {
        // A control character is what a payload spells with a \uXXXX escape; a long body puts the cut inside one.
        val text = "before" + '\u0001' + "after".repeat(PER_MESSAGE_CHARS)
        val stored = Entry("e-escape", 4, "a", rev = 4, kind = "agent", data = buildJsonObject { put("text", text) })
        val payload = WireJson.encodeToString(stored)
        val escape = payload.indexOf("\\u0001")
        assertTrue("the payload spells the control character as an escape", escape > 0)
        // Every place a bounded read can end inside the escape: after the backslash, the u, and each digit but the last.
        for (cut in escape..escape + 5) {
            val closed = closedPayloadSlice(payload.substring(0, cut))
            val decoded = WireJson.decodeFromString<Entry>(closed)
            assertEquals("cut at $cut stays the row", "e-escape", decoded.id)
            assertEquals("cut at $cut keeps the text before the escape", "before", decoded.text)
        }
        // A character a payload spells as a surrogate pair: a cut inside the second escape keeps the pair out of the
        // string, since half of it cannot be encoded back.
        val pairPayload = """{"id":"e-pair","seq":5,"botId":"a","rev":5,"kind":"agent","data":{"text":"\uD83D\uDE00 end"}}"""
        val low = pairPayload.indexOf("""\uDE00""")
        assertTrue("the pair's second escape is where this cuts", low > 0)
        for (cut in low until low + 6) {
            val decoded = WireJson.decodeFromString<Entry>(closedPayloadSlice(pairPayload.substring(0, cut)))
            assertEquals("cut at $cut stays the row", "e-pair", decoded.id)
        }
    }

    @Test fun `a whole row that will not decode reads as unreadable, not as a cut of itself`() {
        val stored = Entry("e-broken", 6, "a", rev = 6, kind = "agent", data = buildJsonObject { put("text", "hello") })
        val whole = WireJson.encodeToString(stored).dropLast(1)
        assertTrue("the row does not decode as it is stored", runCatching { WireJson.decodeFromString<Entry>(whole) }.isFailure)
        assertTrue("a whole row is not passed off as the cut front of itself", runCatching { storedTextOfPayload(whole, "e-broken", complete = true) }.isFailure)
        // The same text, read as the front of a row the read cut: that is the sheet's cut, and it says so.
        assertEquals("hello$STORED_CUT_MARK", storedTextOfPayload(whole, "e-broken", complete = false))
    }

    @Test fun `a closed slice is whole JSON again`() {
        val stored = Entry("e-close", 3, "a", rev = 3, kind = "tool", data = buildJsonObject {
            put("title", "Edit")
            putJsonObject("thread") { put("id", "thread-1") }
            put("diffs", buildJsonArray { add(buildJsonObject { put("path", "src/a.kt"); put("patch", "+ line") }) })
        })
        val payload = WireJson.encodeToString(stored)
        // Cut inside a string, inside a name, and between two values: each closes into a row that decodes.
        for (cut in listOf(payload.indexOf("+ line") + 2, payload.indexOf("thread-1") + 3, payload.lastIndexOf('}') - 1)) {
            val closed = closedPayloadSlice(payload.substring(0, cut))
            val decoded = WireJson.decodeFromString<Entry>(closed)
            assertEquals(closed, "e-close", decoded.id)
            assertEquals(closed, 3L, decoded.seq)
        }
    }
}
