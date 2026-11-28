package com.codync.android.core

import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

class AutoOrderEventTest {
    private fun decode(json: String) = MirrorEvent.decode(WireJson.parseToJsonElement(json).jsonObject)

    @Test fun theSwitchFromAnotherDeviceIsHeard() {
        assertEquals(MirrorEvent.AutoOrder(true), decode("""{"type":"autoOrder","enabled":true}"""))
        assertEquals(MirrorEvent.Ignored, decode("""{"type":"autoOrder"}"""))
        assertEquals(MirrorEvent.HopReview(true), decode("""{"type":"hopReview","enabled":true}"""))
        assertEquals(MirrorEvent.Ignored, decode("""{"type":"hopReview"}"""))
    }

    @Test fun aModelListTheComputerReadIsHeard() {
        assertEquals(MirrorEvent.Models("codex", null, AgentModels(listOf(AgentModel("gpt", "GPT")))),
            decode("""{"type":"models","backend":"codex","loginProfile":null,"models":{"models":[{"id":"gpt","name":"GPT"}]}}"""))
        assertEquals(MirrorEvent.Ignored, decode("""{"type":"models","backend":"codex"}"""))
    }
}
