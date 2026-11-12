package com.codync.android.core

import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

class AutoOrderEventTest {
    private fun decode(json: String) = MirrorEvent.decode(WireJson.parseToJsonElement(json).jsonObject)

    @Test fun theSwitchFromAnotherDeviceIsHeard() {
        assertEquals(MirrorEvent.AutoOrder(true), decode("""{"type":"autoOrder","enabled":true}"""))
        assertEquals(MirrorEvent.Ignored, decode("""{"type":"autoOrder"}"""))
    }
}
