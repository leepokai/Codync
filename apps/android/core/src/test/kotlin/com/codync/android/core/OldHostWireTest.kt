package com.codync.android.core

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

// Hosts before 2.51.0: goes when MIN_HOST reaches 2.51.0.
class OldHostWireTest {
    private fun json(text: String) = Json.parseToJsonElement(text).jsonObject

    @Test fun `a host speaks threads from 2_51_0, and one whose version isn't known counts as current`() {
        assertFalse(speaksThreads("2.50.9"))
        assertTrue(speaksThreads("2.51.0"))
        assertTrue(speaksThreads("2.51.1-dev"))
        assertTrue(speaksThreads(null))
        assertTrue(speaksThreads("soon"))
    }

    @Test fun `what names a task or a bot's threads goes out under both names, whatever the host's version`() {
        // An old version left from the last link, a current one and none yet: a thread's message never goes to the chat.
        for (version in listOf("2.50.1", "2.51.0", null)) {
            fun sent(method: String, body: String) = hostCallFor(version, method, json(body))
            for (method in listOf("send", "steer", "unqueue", "workFirst", "dropWork", "stopWork", "retryWork"))
                assertEquals(method to json("""{"botId":"b","taskId":"w1","itemId":"w1"}"""), sent(method, """{"botId":"b","taskId":"w1"}"""))
            assertEquals("workCutIn" to json("""{"botId":"b","taskId":"w1","insteadOf":"w2","itemId":"w1"}"""), sent("workCutIn", """{"botId":"b","taskId":"w1","insteadOf":"w2"}"""))
            for (method in listOf("updateBot", "createBot"))
                assertEquals(method to json("""{"id":"b","maxThreads":4,"threadWorktrees":false,"maxAgents":4,"taskWorktrees":false}"""),
                    sent(method, """{"id":"b","maxThreads":4,"threadWorktrees":false}"""))
            // A message to the chat, and any other call, goes as it is.
            assertEquals("send" to json("""{"botId":"b","text":"hi"}"""), sent("send", """{"botId":"b","text":"hi"}"""))
            assertEquals("routines" to json("""{"botId":"b"}"""), sent("routines", """{"botId":"b"}"""))
        }
    }

    @Test fun `a thread's transcript is asked for by the host's version`() {
        val asked = json("""{"botId":"b","threadId":"task-1"}""")
        assertEquals("taskEntries" to json("""{"botId":"b","taskId":"task-1"}"""), hostCallFor("2.50.0", "threadEntries", asked))
        for (version in listOf("2.51.0", null)) assertEquals("threadEntries" to asked, hostCallFor(version, "threadEntries", asked))
    }

    @Test fun `what comes in from an older host reads as tasks and threads`() {
        val bot = HostClient.decode(json("""{"id":"b","name":"Bot","rev":1,"maxAgents":4,"taskWorktrees":false,"work":[
            {"id":"w1","status":"working","task":"task-1"},{"id":"w2","status":"waiting","task":null}]}"""))
        assertEquals(4, bot.maxThreads)
        assertEquals(false, bot.threadWorktrees)
        assertEquals(listOf("task-1", null), bot.work.map { it.thread })

        val entry = Entry("e1", 3, "task-1", 9, "tool")
        assertEquals(MirrorEvent.ThreadEntryChanged("task-1", entry),
            MirrorEvent.decode(json("""{"type":"taskEntry","rev":9,"task":"task-1","entry":{"id":"e1","seq":3,"botId":"task-1","rev":9,"kind":"tool"}}""")))

        // A card a thread put in the chat: `task` the object is the thread, `item` the task's id.
        fun data(data: String) = Entry.decode(json("""{"id":"c","seq":1,"botId":"b","rev":1,"kind":"permission","data":$data}""")).data
        val named = json("""{"title":"Run","thread":{"id":"task-1","title":"Fix login"},"task":"w1"}""")
        assertEquals(named, data("""{"title":"Run","task":{"id":"task-1","title":"Fix login"},"item":"w1"}"""))
        assertEquals(json("""{"thread":{"id":"task-1","title":"Fix login"}}"""), data("""{"task":{"id":"task-1","title":"Fix login"}}"""))
        assertEquals(json("""{"task":"w1"}"""), data("""{"item":"w1"}"""))
        // A current host's stays as it is, and so does the chat's own.
        assertEquals(named, data(named.toString()))
        assertEquals(JsonObject(emptyMap()), data("{}"))
    }
}
