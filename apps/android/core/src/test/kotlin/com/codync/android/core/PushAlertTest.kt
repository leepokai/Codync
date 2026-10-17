package com.codync.android.core

import org.junit.Assert.*
import org.junit.Test

class PushAlertTest {
    private fun face(id: String, kind: String = "agent", members: List<String> = emptyList()) =
        """{"id":"$id","kind":"$kind","members":${members.joinToString(",", "[", "]") { "\"$it\"" }},"name":"${id.uppercase()}","avatarColor":"green","avatarShape":"hex"}"""

    @Test fun `a bot's alert names the bot as the sender`() {
        val alert = PushAlert.decode("""{"title":"Reviewer","subtitle":"Task complete","body":"All tests pass","faces":[${face("rev")}]}""")
        assertEquals("Task complete", alert.subtitle)
        val conversation = requireNotNull(alert.conversation("rev"))
        assertEquals("REV", conversation.chat.name)
        assertEquals("hex", conversation.chat.avatarShape)
        assertNull(conversation.speaker)
    }

    @Test fun `a group's alert takes its speaker from from and its members from the faces`() {
        val alert = PushAlert.decode("""{"title":"B in G","subtitle":"Task complete","body":"Done","from":"b",
            "faces":[${face("g", "group", listOf("a", "b", "gone"))},${face("a")},${face("b")}]}""")
        val conversation = requireNotNull(alert.conversation("g"))
        assertEquals(listOf("a", "b"), conversation.members.map { it.id })
        assertEquals("b", conversation.speaker?.id)
        assertNull(PushAlert.decode("""{"title":"G","body":"x","faces":[${face("g", "group", listOf("a"))}]}""").conversation("g")?.speaker)
    }

    @Test fun `an alert without its chat's face stays plain and long text is bounded`() {
        val alert = PushAlert.decode("""{"title":"${"t".repeat(200)}","body":"${"b".repeat(600)}","unknown":1}""")
        assertNull(alert.conversation("rev"))
        assertNull(alert.subtitle)
        assertEquals(120, alert.title.length)
        assertEquals(400, alert.body.length)
    }
}
