package com.codync.android.core

import kotlinx.serialization.json.decodeFromJsonElement
import org.junit.Assert.*
import org.junit.Test

class MemoryTest {
    @Test fun `a bot's memory decodes both layers, dreams, notes and the harness's own notes`() {
        val memory = WireJson.decodeFromJsonElement<BotMemory>(WireJson.parseToJsonElement("""{
            "shared": true,
            "yours": {"entries": [{"id": 4503599627370495, "bot": null, "state": "active", "text": "Lives in Taipei", "author": "user",
                "authorBot": null, "pinned": false, "evidence": [], "createdAt": 1, "updatedAt": 2}], "used": 12, "cap": 900},
            "bot": {"entries": [{"id": 7, "bot": "b", "state": "archived", "text": "Prefers tabs", "author": "bot", "authorBot": "b", "updatedAt": 3}],
                "used": 4, "cap": 600},
            "files": {"yours": {"files": [], "folder": "/m"}, "bot": {"files": [{"bot": "b", "name": "release.md", "description": "How releases go",
                "tokens": 812, "deleted": false, "updatedAt": 5}], "folder": "/m/b"}, "cap": 800, "folderCap": 20},
            "dreams": [{"id": 1, "at": 9, "undone": false, "added": [{"id": 8, "text": "Ships on Fridays"}], "archived": [],
                "files": [{"name": "release.md", "shared": false, "version": 2}]}],
            "projectMemory": {"path": "/home/me/.claude/projects/x/memory/MEMORY.md", "text": ""},
            "memoryRev": 41}"""))
        assertTrue(memory.shared)
        // Fact IDs are 52-bit numbers.
        assertEquals(4503599627370495, memory.yours.entries.single().id)
        assertTrue(memory.bot.entries.single().archived)
        assertEquals(812, memory.files?.bot?.files?.single()?.tokens)
        assertEquals("release.md", memory.dreams.single().files?.single()?.name)
        assertEquals("", memory.projectMemory?.text)
    }

    @Test fun `a host that keeps no harness notes or files still decodes`() {
        val memory = WireJson.decodeFromJsonElement<BotMemory>(WireJson.parseToJsonElement("""{"shared": false,
            "yours": {"entries": [], "used": 0, "cap": 900}, "bot": {"entries": [], "used": 0, "cap": 600}, "dreams": [], "projectMemory": null}"""))
        assertNull(memory.projectMemory)
        assertNull(memory.files)
    }
}
