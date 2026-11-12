package com.codync.android

import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Test

class ConversationStampTest {
    @Test fun `the conversation is stamped with the host store schema, or its version before it sent one`() {
        assertEquals("schema 8", conversationStamp(buildJsonObject { put("schema", 8); put("version", "2.45.1") }))
        assertEquals("2.45.0", conversationStamp(buildJsonObject { put("version", "2.45.0") }))
        assertEquals("2.45.0", conversationStamp(buildJsonObject { put("schema", JsonNull); put("version", "2.45.0") }))
    }
}
