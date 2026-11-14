package com.codync.android

import com.codync.android.core.ConversationFind
import com.codync.android.core.Entry
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

/** iOS `findSearchesOnlyMessageText`. */
class FindableTest {
    private fun entry(seq: Long, kind: String, text: String, final: Boolean? = null) = Entry("e$seq", seq, "b", seq, kind, 1,
        JsonObject(mapOf("text" to JsonPrimitive(text)) + listOfNotNull(final?.let { "final" to JsonPrimitive(it) })), createdAt = seq)

    @Test fun `find searches only what messages say`() {
        val items = chatItems(listOf(entry(1, "user", "find the plan"), entry(3, "agent", "The plan, and the Plan", final = true),
            entry(4, "notice", "plan saved")), emptyList())
        val found = findable(items)
        assertEquals(listOf("find the plan", "The plan, and the Plan"), found.map { it.second })
        assertEquals(3, ConversationFind.matches(found, "plan").size)
    }

    @Test fun `the find events carry only whether something was found`() {
        assertEquals("conversation_find_opened", AnalyticsEvent.ConversationFindOpened.name)
        assertEquals("{}", JsonObject(AnalyticsEvent.ConversationFindOpened.properties).toString())
        assertEquals("""{"found":true}""", JsonObject(AnalyticsEvent.ConversationFindSearched(true).properties).toString())
    }
}
