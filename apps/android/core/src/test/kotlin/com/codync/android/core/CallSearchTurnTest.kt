package com.codync.android.core

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class CallSearchTurnTest {
    private fun content(json: String): JsonObject = Json.parseToJsonElement(json).jsonObject

    @Test fun lateTranscriptionStaysWithItsTurnAndKeepsTheSuggestions() {
        val turn = CallSearchTurn()
        turn.receive(content("""{"modelTurn":{},"groundingMetadata":{"searchEntryPoint":{"renderedContent":"<p>S</p>"}},"turnComplete":true}"""))
        assertEquals(CallSearchResult("<p>S</p>", "Late piece."), turn.receive(content("""{"outputTranscription":{"text":"Late piece."}}""")))
        assertNull(turn.receive(content("""{"modelTurn":{},"outputTranscription":{"text":"Next turn."}}""")))
    }

    @Test fun theUserBeingHeardStartsTheNextTurnSoItsEarlyTranscriptionStays() {
        val turn = CallSearchTurn()
        turn.receive(content("""{"modelTurn":{},"groundingMetadata":{"searchEntryPoint":{"renderedContent":"<p>First</p>"}},"outputTranscription":{"text":"A."},"turnComplete":true}"""))
        assertNull(turn.receive(content("""{"inputTranscription":{"text":"And tomorrow?"}}""")))
        assertNull(turn.receive(content("""{"outputTranscription":{"text":"Tomorrow "}}""")))
        turn.receive(content("""{"modelTurn":{}}"""))
        assertEquals(CallSearchResult("<p>Second</p>", "Tomorrow is sunny."),
            turn.receive(content("""{"groundingMetadata":{"searchEntryPoint":{"renderedContent":"<p>Second</p>"}},"outputTranscription":{"text":"is sunny."}}""")))
        assertEquals(2, turn.answers)
    }
}
