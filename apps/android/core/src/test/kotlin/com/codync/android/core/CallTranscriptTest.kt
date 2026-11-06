package com.codync.android.core

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class CallTranscriptTest {
    @Test fun invalidNulTextDoesNotLoseTheReceipt() {
        val capture = CallReceiptTranscript()
        capture.add("user", "recognized", "\u0000")
        capture.add("user", "recognized", "hello\u0000 there", "user")
        capture.add("user", "recognized", "\u0000!", "user", delta = true)
        val snapshot = capture.snapshot()
        assertTrue(snapshot["truncated"]!!.jsonPrimitive.boolean)
        val entries = snapshot["entries"]!!.jsonArray
        assertEquals(1, entries.size)
        assertEquals("hello there!", entries[0].jsonObject["text"]!!.jsonPrimitive.content)
    }
    @Test fun providerTextNeverClaimsPlayback() {
        val capture = CallReceiptTranscript()
        val event = buildJsonObject {
            put("type", "response.output_audio_transcript.done"); put("item_id", "item"); put("content_index", 0); put("transcript", "Hello")
        }
        capture.openAI(event); capture.openAI(event)
        val entries = capture.snapshot()["entries"]!!.jsonArray
        assertEquals(1, entries.size)
        assertEquals("generated", entries[0].jsonObject["source"]!!.jsonPrimitive.content)
    }
    @Test fun limitsPreserveUnicode() {
        val capture = CallReceiptTranscript()
        capture.add("user", "recognized", "a".repeat(3999) + "😀")
        val snapshot = capture.snapshot()
        assertTrue(snapshot["truncated"]!!.jsonPrimitive.boolean)
        assertEquals(3999, snapshot["entries"]!!.jsonArray[0].jsonObject["text"]!!.jsonPrimitive.content.length)
    }
}
