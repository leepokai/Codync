package com.codync.android

import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

class PermissionCardTest {
    private fun option(id: String, name: String, kind: String) = JsonObject(
        mapOf(
            "optionId" to JsonPrimitive(id),
            "name" to JsonPrimitive(name),
            "kind" to JsonPrimitive(kind),
        )
    )

    @Test fun `duplicate allow_always options keep distinct agent names`() {
        val options = listOf(
            option("once", "Yes", "allow_once"),
            option("session", "Always allow for this session", "allow_always"),
            option("forever", "Always allow", "allow_always"),
            option("deny", "No", "reject_once"),
        )
        assertEquals("Allow once", permissionOptionLabel(options[0], options) { it })
        assertEquals("Always allow for this session", permissionOptionLabel(options[1], options) { it })
        assertEquals("Always allow", permissionOptionLabel(options[2], options) { it })
        assertEquals("Deny", permissionOptionLabel(options[3], options) { it })

        val answered = JsonObject(
            mapOf(
                "status" to JsonPrimitive("answered"),
                "selected" to JsonPrimitive("session"),
                "options" to JsonArray(options),
            )
        )
        assertEquals("Always allow for this session", permissionOutcomeLabel(answered) { it })
    }

    @Test fun `single allow_always keeps localized label`() {
        val options = listOf(
            option("once", "Yes", "allow_once"),
            option("always", "Yes, always", "allow_always"),
        )
        assertEquals("Always allow", permissionOptionLabel(options[1], options) { it })

        val answered = JsonObject(
            mapOf(
                "status" to JsonPrimitive("answered"),
                "selected" to JsonPrimitive("always"),
                "options" to JsonArray(options),
            )
        )
        assertEquals("Always allowed", permissionOutcomeLabel(answered) { it })
    }
}
