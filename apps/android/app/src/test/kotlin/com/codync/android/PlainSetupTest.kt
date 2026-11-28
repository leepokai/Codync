package com.codync.android

import com.codync.android.core.Login
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

class PlainSetupTest {
    @Test fun `a thread without a bot names its whole setup`() {
        val setup = plainSetup(Login("claude", "work"), "/Users/me", null, mapOf("effort" to "high"))
        assertEquals(JsonPrimitive("claude"), setup["backend"])
        assertEquals(JsonPrimitive("work"), setup["loginProfile"])
        assertEquals(JsonPrimitive("/Users/me"), setup["cwd"])
        val model = setup["model"]!!.jsonObject
        assertFalse("id" in model)
        assertEquals(buildJsonObject { put("effort", "high") }, model["options"])
        assertEquals(buildJsonObject { put("permission", "ask"); put("mode", "auto") }, setup["approval"])
    }

    @Test fun `the default login and an unknown home are left to the host`() {
        val setup = plainSetup(Login("codex"), "", "gpt", emptyMap())
        assertEquals(JsonPrimitive(""), setup["loginProfile"])
        assertFalse("cwd" in setup)
        assertEquals(JsonPrimitive("gpt"), setup["model"]!!.jsonObject["id"])
    }
}
