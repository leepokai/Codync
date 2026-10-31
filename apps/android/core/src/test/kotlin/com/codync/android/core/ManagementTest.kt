package com.codync.android.core

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class ManagementTest {
    @Test fun `group edits preserve agent settings and member order in existing host contracts`() {
        val roster = listOf(Bot("a", "Ada"), Bot("b", "Grace"))
        val draft = GroupDraft(members = listOf("b", "a", "b"), pinned = true)
        val payload = draft.body(roster)
        assertEquals("Grace, Ada", payload.getValue("name").jsonPrimitive.content)
        assertEquals(listOf(JsonPrimitive("b"), JsonPrimitive("a")), payload.getValue("members").jsonArray.toList())
        assertEquals("group", payload.getValue("kind").jsonPrimitive.content)
        for (key in listOf("backend", "cwd", "connectors", "skills", "permission", "model", "notify", "hidden")) assertFalse(key in payload)
        val existing = Bot("group", "Team", kind = "group", members = listOf("a"), pinned = true)
        assertEquals("group", GroupDraft.from(existing).body(roster).getValue("id").jsonPrimitive.content)
    }

    @Test fun `unavailable members and nested groups cannot silently change a saved roster`() {
        for (members in listOf(emptyList(), listOf("missing"), listOf("nested"), listOf("deleted"))) {
            assertFails<IllegalArgumentException> { GroupDraft(members = members).body(listOf(
                Bot("nested", "Team", kind = "group"), Bot("deleted", "Gone", deleted = true))) }
        }
    }
}
