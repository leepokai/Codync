package com.codync.android.core

import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.*
import org.junit.Test

class ReviewTest {
    private val day = 86_400_000L

    private fun user(nonce: String, turn: Long) = Entry("u$turn", turn, "b", turn, "user", turn,
        buildJsonObject { put("text", "do it"); put("clientNonce", nonce); put("status", "sent") })
    private fun answer(turn: Long) = Entry("a$turn", turn, "b", turn, "agent", turn, buildJsonObject { put("text", "done"); put("final", true) })
    /** A message from this phone that isn't the person asking: a call's, a routine's or another bot's. */
    private fun relayed(nonce: String, field: String) = Entry("r1", 1, "b", 1, "user", 1,
        buildJsonObject { put("text", "do it"); put("clientNonce", nonce); put("status", "sent"); put(field, "x") })

    /** An invitation with an opportunity waiting, made on day 10 as the person came back from "chat". */
    private fun waiting(events: MutableList<ReviewInvitation.Event>): Pair<ReviewInvitation, Long> {
        val invitation = ReviewInvitation(ReviewHistory(firstUse = 0, completed = 4, activeDays = setOf("a", "b", "c")), {}) { events += it }
        invitation.setActive(true, 10 * day)
        invitation.sent("n1", 10 * day)
        invitation.viewed("chat", listOf(user("n1", 1), answer(1)), 10 * day + 1_000)
        invitation.returned("chat", "2.34.0", 10 * day + 120_000)
        return invitation to requireNotNull(invitation.opportunity)
    }

    @Test fun `only a final answer to this phone's own message finishes it`() {
        val sent = mapOf("n1" to 0L)
        assertEquals(listOf("n1"), ReviewCompletion.match(listOf(user("n1", 1), answer(1)), sent, day))
        // Still working, or a notice in the turn (a stop, a failure).
        assertNull(ReviewCompletion.match(listOf(user("n1", 1)), sent, day))
        val notice = Entry("x", 2, "b", 2, "notice", 1, buildJsonObject { put("text", "Stopped.") })
        assertNull(ReviewCompletion.match(listOf(user("n1", 1), notice, answer(1)), sent, day))
        // Another device's message, or one sent over a month ago.
        assertNull(ReviewCompletion.match(listOf(user("n2", 1), answer(1)), sent, day))
        assertNull(ReviewCompletion.match(listOf(user("n1", 1), answer(1)), sent, 31 * day))
    }

    @Test fun `eligible after a week, three active days and five answers, once per version`() {
        var history = ReviewHistory(firstUse = 0, completed = 5)
        for (d in 0 until 3) history = history.recordingActivity(d * day + 1, java.time.ZoneOffset.UTC)
        assertFalse(history.eligible(6 * day, "2.34.0"))
        assertTrue(history.eligible(7 * day, "2.34.0"))
        history = history.recordingRequest(7 * day, "2.34.0")
        assertFalse(history.eligible(200 * day, "2.34.0"))
        // Another version needs 120 days and five more answers.
        assertFalse(history.eligible(100 * day, "2.35.0"))
        assertTrue(history.copy(completed = 10).eligible(130 * day, "2.35.0"))
    }

    @Test fun `returning from the chat whose answer was read makes one opportunity, and asking uses it`() {
        val events = mutableListOf<ReviewInvitation.Event>()
        var history = ReviewHistory(firstUse = 0, completed = 4, activeDays = setOf("a", "b", "c"))
        val invitation = ReviewInvitation(history, { history = it }) { events += it }
        val now = 10 * day
        invitation.setActive(true, now)
        invitation.sent("n1", now)
        invitation.viewed("chat", listOf(user("n1", 1), answer(1)), now + 1_000)
        invitation.returned("other", "2.34.0", now + 120_000)
        assertNull(invitation.opportunity)
        invitation.viewed("chat", listOf(user("n1", 1), answer(1)), now + 1_000)
        invitation.sent("n2", now + 2_000)
        invitation.viewed("chat", listOf(user("n2", 2), answer(2)), now + 3_000)
        invitation.returned("chat", "2.34.0", now + 120_000)
        val token = requireNotNull(invitation.opportunity)
        assertTrue(invitation.request(token, "2.34.0", now + 122_000))
        assertFalse(invitation.request(token, "2.34.0", now + 123_000))
        assertEquals(listOf(ReviewInvitation.Event.Opportunity(6), ReviewInvitation.Event.Requested(6)), events)
        assertEquals(setOf("2.34.0"), history.requestedVersions)
    }

    @Test fun `twice in a rolling year at most`() {
        val history = ReviewHistory(firstUse = 0, completed = 15, activeDays = setOf("a", "b", "c"),
            requests = listOf(ReviewHistory.Request(0, "2.30.0", 5), ReviewHistory.Request(130 * day, "2.31.0", 10)),
            requestedVersions = setOf("2.30.0", "2.31.0"))
        assertFalse(history.eligible(260 * day, "2.32.0"))
        assertTrue(history.eligible(366 * day, "2.32.0"))
    }

    @Test fun `leaving the app, a touch or something in use turns the opportunity down`() {
        val events = mutableListOf<ReviewInvitation.Event>()
        val (left, _) = waiting(events)
        left.setActive(false, 10 * day + 121_000)
        assertNull(left.opportunity)
        val (touched, _) = waiting(events)
        touched.cancel(ReviewCancellation.Interaction)
        val (busy, token) = waiting(events)
        busy.interrupt(Any(), true)
        assertFalse(busy.request(token, "2.34.0", 10 * day + 122_000))
        assertEquals(listOf(ReviewCancellation.Background, ReviewCancellation.Interaction, ReviewCancellation.Unavailable),
            events.filterIsInstance<ReviewInvitation.Event.Cancelled>().map { it.reason })
        // An answer read while something is in use doesn't count.
        busy.sent("n2", 10 * day + 130_000)
        busy.viewed("chat", listOf(user("n2", 2), answer(2)), 10 * day + 131_000)
        assertEquals(5, busy.history.completed)
    }

    @Test fun `a call's, a routine's or another bot's turn never counts`() {
        val sent = mapOf("n1" to 0L)
        for (field in listOf("call", "routineId", "from")) assertNull(field, ReviewCompletion.match(listOf(relayed("n1", field), answer(1)), sent, day))
    }

    @Test fun `sent messages are forgotten after a month and kept to a hundred`() {
        var history = ReviewHistory(firstUse = 0, sent = mapOf("old" to 0L, "recent" to 20 * day))
        val invitation = ReviewInvitation(history, { history = it }) { }
        invitation.setActive(true, 31 * day)
        assertEquals(setOf("recent"), history.sent.keys)
        for (i in 0 until 100) invitation.sent("n$i", 31 * day + i)
        assertEquals(100, history.sent.size)
        assertFalse("recent" in history.sent)
    }

    @Test fun `a turn with several messages counts once`() {
        var history = ReviewHistory(firstUse = 0)
        val invitation = ReviewInvitation(history, { history = it }) { }
        invitation.setActive(true, day)
        invitation.sent("n1", day)
        invitation.sent("n2", day + 1)
        val second = Entry("u1b", 2, "b", 2, "user", 1, buildJsonObject { put("text", "also"); put("clientNonce", "n2"); put("status", "sent") })
        invitation.viewed("chat", listOf(user("n1", 1), second, answer(1)), day + 1_000)
        assertEquals(1, history.completed)
        assertTrue(history.sent.isEmpty())
    }

    @Test fun `play version codes read back as versions and a newer one is reminded until dismissed`() {
        assertEquals("2.34.0", UpdateReminder.versionOf(2_034_000))
        assertEquals("2.34.12", UpdateReminder.versionOf(2_034_012))
        assertEquals("2.35.0", UpdateReminder.app("2.34.0", "2.35.0", null))
        assertNull(UpdateReminder.app("2.34.0", "2.35.0", "2.35.0"))
        assertNull(UpdateReminder.app("2.35.0", "2.35.0", null))
        assertNull(UpdateReminder.app("2.34.0", null, null))
    }
}
