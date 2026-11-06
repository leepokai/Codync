package com.codync.android.core

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class ConversationTest {
    private fun entry(kind: String, final: Boolean = false) = Entry("e", 1, "b", rev = 1, kind = kind,
        data = buildJsonObject { put("text", "message"); put("final", final) })

    @Test fun `a bot works in the one chat its turn talks in`() {
        val bot = Bot("b", status = "working")
        assertTrue(bot.isWorking("b"))
        assertTrue(bot.copy(workingChat = "group").isWorking("group"))
        assertFalse(bot.copy(workingChat = "group").isWorking("b"))
        assertFalse(bot.copy(status = "idle").isWorking("b"))
    }

    @Test fun `users and final replies can be quoted, and the quote reads leniently`() {
        assertTrue(entry("user").isReplyable)
        assertTrue(entry("agent", true).isReplyable)
        for (kind in listOf("agent", "notice", "permission", "tool")) assertFalse(entry(kind).isReplyable)
        val reply = Entry("r", 2, "b", rev = 2, kind = "user", data = buildJsonObject {
            put("text", "hi"); put("replyTo", buildJsonObject { put("id", "e"); put("author", "b"); put("text", "message") })
        })
        assertEquals(ReplyQuote("e", "b", "message"), reply.replyTo)
        assertNull(entry("user").replyTo)
        assertNull(Entry("x", 3, "b", rev = 3, kind = "user", data = buildJsonObject { put("replyTo", "e") }).replyTo)
    }

    @Test fun `a sending reply quotes who wrote the message and falls back to its file names`() {
        assertEquals(ReplyQuote("e", "user", "message"), entry("user").replyQuote)
        assertEquals("b", entry("agent", true).replyQuote.author)
        val member = Entry("m", 2, "g", rev = 2, kind = "agent", data = buildJsonObject { put("text", "hey"); put("final", true); put("author", "ann") })
        assertEquals("ann", member.replyQuote.author)
        val files = Entry("f", 3, "b", rev = 3, kind = "user", data = buildJsonObject {
            put("text", "")
            putJsonArray("attachments") { addJsonObject { put("id", "a"); put("name", "a.png"); put("size", 1) }; addJsonObject { put("id", "c"); put("name", "c.pdf"); put("size", 1) } }
        })
        assertEquals("a.png, c.pdf", files.replyQuote.text)
        val long = Entry("l", 4, "b", rev = 4, kind = "user", data = buildJsonObject { put("text", "x".repeat(900)) })
        assertEquals(500, long.replyQuote.text.length)
    }

    @Test fun `a send carries the message it quotes and its files`() {
        val send = PendingSend("n", "b", "Ship it", 1, replyTo = ReplyQuote("e", "user", "Ready?"))
        assertEquals(buildJsonObject { put("botId", "b"); put("text", "Ship it"); put("clientNonce", "n"); put("replyTo", "e")
            putJsonArray("attachments") { add("u1") } }, send.body(listOf("u1")))
        assertFalse("replyTo" in send.copy(replyTo = null).body())
    }

    @Test fun `chat shows final replies permissions notices and users without narration`() {
        assertTrue(entry("user").isChat)
        assertTrue(entry("permission").isChat)
        assertTrue(entry("notice").isChat)
        assertTrue(entry("agent", true).isChat)
        for (kind in listOf("agent", "thought", "tool", "plan")) assertFalse(entry(kind).isChat)
        for (key in listOf("memory", "memoryFile", "compaction")) {
            val line = Entry("m", 1, "b", rev = 1, kind = "notice", data = buildJsonObject { put("text", "Remembered: x"); put(key, buildJsonObject {}) })
            assertFalse("an older host's memory line", line.isChat)
        }
        for ((status, shown) in listOf("kept" to false, "sent" to true)) {
            val handoff = Entry("h", 1, "b", rev = 1, kind = "notice", data = buildJsonObject { put("handoff", buildJsonObject { put("status", status) }) })
            assertEquals(status, shown, handoff.isChat)
        }
    }

    @Test fun `malformed known events fail before a checkpoint while unknown events are ignored`() {
        assertFails<IllegalArgumentException> { MirrorEvent.decode(buildJsonObject { put("type", "entry"); put("entry", buildJsonObject { put("id", "broken") }) }) }
        assertEquals(MirrorEvent.Ignored, MirrorEvent.decode(buildJsonObject { put("type", "future"); put("rev", 999) }))
    }

    @Test fun `templates preserve portable settings and exclude identity and runtime flags`() {
        val bot = Bot("identity", "Reviewer", description = "Review carefully", model = "chosen", connectors = listOf("c"),
            skills = listOf("s"), pinned = true, hidden = true, managedWorkspace = true, cwd = "/host/workspace", rev = 77, section = "Work")
        val template = WireJson.parseToJsonElement(BotDraft.from(bot).template()).jsonObject
        for (key in listOf("id", "pinned", "hidden", "rev", "status", "autoName", "createdAt", "section")) assertFalse(key in template)
        // Saving the editor leaves the section to the roster's menu: updateBot keeps a field it isn't sent.
        assertFalse("section" in BotDraft.from(bot).body())
        assertEquals("", template.getValue("cwd").jsonPrimitive.content)
        assertEquals("chosen", template.getValue("model").jsonPrimitive.content)
        assertEquals(listOf(JsonPrimitive("c")), template.getValue("connectors").jsonArray.toList())
        assertEquals(JsonNull, BotDraft.from(bot).copy(model = null).body()["model"])
    }

    @Test fun `a bot decodes its roster section, and an older host's without one`() {
        assertEquals("Work", HostClient.decode(WireJson.parseToJsonElement("""{"id":"b","name":"Scout","section":"Work"}""").jsonObject).section)
        assertNull(HostClient.decode(WireJson.parseToJsonElement("""{"id":"b","name":"Scout"}""").jsonObject).section)
    }

    @Test fun `a settings draft takes changes made elsewhere and keeps its own edits`() {
        val base = BotDraft.from(Bot("b", "Lead", model = "sonnet"))
        val fresh = base.copy(model = "opus", name = "Boss", manageTeam = true)
        assertEquals(fresh.copy(notify = false), base.copy(notify = false).rebased(base, fresh))
        assertEquals("haiku", base.copy(model = "haiku").rebased(base, fresh).model)
    }

    @Test fun `malformed rendered fields cannot enter the mirror`() {
        for (data in listOf("{\"text\":{}}", "{\"final\":[]}", "{\"options\":[{}]}",
            "{\"reactions\":[{}]}", "{\"attachments\":[{\"id\":\"id\",\"name\":\"file\",\"size\":-1}]}")) {
            val value = WireJson.parseToJsonElement("""{"id":"e","botId":"b","seq":1,"rev":1,"kind":"user","data":$data}""").jsonObject
            assertFails<IllegalArgumentException> { Entry.decode(value) }
        }
    }
}
