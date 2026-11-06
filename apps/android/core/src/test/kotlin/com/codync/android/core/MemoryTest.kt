package com.codync.android.core

import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class MemoryTest {
    private val view = """{"shared":true,
        "yours":{"entries":[{"id":1,"state":"active","text":"Lives in Taipei","author":"user","pinned":true,"updatedAt":5}],"used":10,"cap":400},
        "bot":{"entries":[{"id":2,"bot":"b","state":"archived","text":"Likes tea","author":"bot","authorBot":"b","pinned":false,"updatedAt":6,"future":1}],"used":3,"cap":800},
        "dreams":[{"id":9,"at":7,"undone":false,"added":[{"id":3,"text":"x"}],"archived":[{"id":2,"text":"y"}],"files":[{"name":"people.md","shared":false,"version":4}]}],
        "projectMemory":{"path":"/p","text":"notes"},
        "files":{"yours":{"files":[],"folder":"/s"},"bot":{"files":[{"name":"a.md","hash":"h","tokens":12,"deleted":false,"updatedAt":1}],"folder":"/b"},"cap":2000,"folderCap":20}}"""

    @Test fun `a bot's memory decodes its layers dreams notes and the harness notes`() {
        val memory = WireJson.decodeFromString<BotMemory>(view)
        assertTrue(memory.shared)
        assertEquals("Lives in Taipei", memory.yours.entries.single().text)
        assertTrue(memory.bot.entries.single().archived)
        assertEquals("people.md", memory.dreams.single().files.orEmpty().single().name)
        assertEquals("notes", memory.projectMemory?.text)
        assertEquals(20, memory.files?.folderCap)
        assertEquals("a.md", memory.files?.bot?.files?.single()?.name)
    }

    @Test fun `an older host's flat listing is not a memory`() {
        assertFails<SerializationException> {
            WireJson.decodeFromString<BotMemory>("""{"facts":[{"id":"a","content":"x"}],"total":1}""")
        }
    }

    @Test fun `memory requests leave unset fields out`() {
        val pin = MemoryCalls.mutation("b", id = 4, pinned = true)
        assertEquals(setOf("botId", "id", "pinned"), pin.keys)
        val clear = MemoryCalls.mutation("b", shared = true)
        assertEquals(setOf("botId", "shared"), clear.keys)
        val save = MemoryCalls.mutation("b", id = null, shared = false, text = "fact")
        assertEquals(setOf("botId", "shared", "text"), save.keys)
    }

    @Test fun `a note change picks save delete or restore and a new note has no base`() {
        assertEquals("saveMemoryFile", MemoryCalls.fileMethod("text", null))
        assertEquals("deleteMemoryFile", MemoryCalls.fileMethod(null, null))
        assertEquals("restoreMemoryFile", MemoryCalls.fileMethod(null, 3))
        val body = MemoryCalls.fileChange("b", "a.md", false, null, "text", null)
        assertEquals(setOf("botId", "file", "shared", "content"), body.keys)
        assertEquals("h", MemoryCalls.fileChange("b", "a.md", true, "h", null, 3).getValue("base").jsonPrimitive.content)
    }

    @Test fun `a bot carries its memory switches and revision`() {
        val bot = HostClient.decode(WireJson.parseToJsonElement(
            """{"id":"b","name":"Bot","sharedMemory":true,"harnessMemory":false,"memoryRev":8}""").jsonObject)
        assertEquals(true, bot.sharedMemory)
        assertEquals(false, bot.harnessMemory)
        assertEquals(8L, bot.memoryRev)
        assertNull(HostClient.decode(WireJson.parseToJsonElement("""{"id":"c","name":"Old"}""").jsonObject).memoryRev)
    }
}
