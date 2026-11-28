package com.codync.android

import com.codync.android.core.Entry
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * The answer rows are Compose, and this module's Compose harness is androidTest, which needs a device.
 * This lane does not install or run on one. [canAnswerStub] is the seam [PermissionCard] uses to draw
 * those rows instead of the stored-approval placeholder. A stub never takes that path.
 */
class PermissionCardAnswerTest {
    @Test fun `a pending stub that carries options stays the placeholder`() {
        assertFalse(canAnswerStub(stub(status = "pending", options = choices("allow", "deny"))))
    }

    @Test fun `a pending stub with no answerable option stays the placeholder`() {
        assertFalse(canAnswerStub(stub(status = "pending", options = null)))
        assertFalse(canAnswerStub(stub(status = "pending", options = buildJsonArray { })))
        assertFalse(canAnswerStub(stub(status = "pending", options = buildJsonArray {
            add(buildJsonObject { put("name", "Allow") })
        })))
        assertFalse(canAnswerStub(stub(status = "pending", options = buildJsonArray {
            add(buildJsonObject { put("optionId", ""); put("name", "Allow") })
        })))
        val textOptions = buildJsonObject {
            put("storedOnly", true)
            put("status", "pending")
            put("options", "allow")
        }
        assertFalse(canAnswerStub(Entry("e-text", 1, "bot", 1, "permission", data = textOptions)))
    }

    @Test fun `an answered stub and a card that was read whole are not answered through the stub seam`() {
        assertFalse(canAnswerStub(stub(status = "answered", options = choices("allow"))))
        assertFalse(canAnswerStub(stub(status = null, options = choices("allow"))))
        assertFalse(canAnswerStub(entry(storedOnly = false, status = "pending", options = choices("allow"))))
    }

    @Test fun `a stub whose row could not be read stays the warning even when options were kept`() {
        val failed = stub(status = "pending", options = choices("allow")).let {
            it.copy(data = JsonObject(it.data + ("approvalReadFailed" to JsonPrimitive(true))))
        }
        assertTrue(failed.approvalReadFailed)
        assertFalse(canAnswerStub(failed))
    }

    private fun choices(vararg ids: String): JsonArray = buildJsonArray {
        for (id in ids) add(buildJsonObject {
            put("optionId", id)
            put("name", id)
            put("kind", "allow_once")
        })
    }

    private fun stub(status: String?, options: JsonArray?) = entry(storedOnly = true, status = status, options = options)

    private fun entry(storedOnly: Boolean, status: String?, options: JsonArray?): Entry {
        val data = buildJsonObject {
            if (storedOnly) put("storedOnly", true)
            put("text", "Large message kept in storage (40000 characters).")
            if (status != null) put("status", status)
            if (options != null) put("options", options)
        }
        return Entry("e-1", 1, "bot", 1, "permission", data = data)
    }
}
