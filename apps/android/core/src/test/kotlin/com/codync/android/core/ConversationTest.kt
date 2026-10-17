package com.codync.android.core

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class ConversationTest {
    private fun entry(kind: String, final: Boolean = false) = Entry("e", 1, "b", rev = 1, kind = kind,
        data = buildJsonObject { put("text", "message"); put("final", final) })

    @Test fun `a bot works in the one chat and thread its turn talks in`() {
        val bot = Bot("b", status = "working")
        assertTrue(bot.isWorking("b", null))
        assertFalse(bot.isWorking("b", "root"))
        assertTrue(bot.copy(workingThread = "root").isWorking("b", "root"))
        assertFalse(bot.copy(workingThread = "root").isWorking("b", null))
        assertTrue(bot.copy(workingChat = "group").isWorking("group", null))
        assertFalse(bot.copy(workingChat = "group").isWorking("b", null))
        assertFalse(bot.copy(status = "idle").isWorking("b", null))
    }

    @Test fun `chat shows final replies permissions notices and users without narration`() {
        assertTrue(entry("user").isChat)
        assertTrue(entry("permission").isChat)
        assertTrue(entry("notice").isChat)
        assertTrue(entry("agent", true).isChat)
        for (kind in listOf("agent", "thought", "tool", "plan")) assertFalse(entry(kind).isChat)
    }

    @Test fun `malformed known events fail before a checkpoint while unknown events are ignored`() {
        assertFails<IllegalArgumentException> { MirrorEvent.decode(buildJsonObject { put("type", "entry"); put("entry", buildJsonObject { put("id", "broken") }) }) }
        assertEquals(MirrorEvent.Ignored, MirrorEvent.decode(buildJsonObject { put("type", "future"); put("rev", 999) }))
    }

    @Test fun `templates preserve portable settings and exclude identity and runtime flags`() {
        val bot = Bot("identity", "Reviewer", description = "Review carefully", model = "chosen", connectors = listOf("c"),
            skills = listOf("s"), pinned = true, hidden = true, managedWorkspace = true, cwd = "/host/workspace", rev = 77)
        val template = WireJson.parseToJsonElement(BotDraft.from(bot).template()).jsonObject
        for (key in listOf("id", "pinned", "hidden", "rev", "status", "autoName", "createdAt")) assertFalse(key in template)
        assertEquals("", template.getValue("cwd").jsonPrimitive.content)
        assertEquals("chosen", template.getValue("model").jsonPrimitive.content)
        assertEquals(listOf(JsonPrimitive("c")), template.getValue("connectors").jsonArray.toList())
        assertEquals(JsonNull, BotDraft.from(bot).copy(model = null).body()["model"])
    }

    @Test fun `malformed rendered fields cannot enter the mirror`() {
        for (data in listOf("{\"text\":{}}", "{\"final\":[]}", "{\"options\":[{}]}",
            "{\"reactions\":[{}]}", "{\"thread\":{\"count\":-1}}", "{\"attachments\":[{\"id\":\"id\",\"name\":\"file\",\"size\":-1}]}")) {
            val value = WireJson.parseToJsonElement("""{"id":"e","botId":"b","seq":1,"rev":1,"kind":"user","data":$data}""").jsonObject
            assertFails<IllegalArgumentException> { Entry.decode(value) }
        }
    }
}
