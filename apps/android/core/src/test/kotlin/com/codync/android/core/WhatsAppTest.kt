package com.codync.android.core

import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class WhatsAppTest {
    @Test fun `a pairing code decodes and shows in two groups of four`() {
        val link = WhatsApp.decode(Json.parseToJsonElement("""{"state":"pairing","pairCode":"ABCD2345"}"""))
        assertEquals("pairing", link.state)
        assertEquals("ABCD2345", link.pairCode)
        assertEquals("ABCD-2345", WhatsApp.groupedCode(link.pairCode.orEmpty()))
    }

    @Test fun `the pairing state asks for the code and an older host has none`() {
        assertEquals("Enter the code on your phone.", WhatsApp.stateText("pairing"))
        assertNull(WhatsApp.decode(Json.parseToJsonElement("""{"state":"starting"}""")).pairCode)
    }
}
