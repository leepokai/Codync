package com.codync.android.core

import org.junit.Assert.*
import org.junit.Test

class ReviewTest {
    private val day = 86_400_000L
    private val now = 1_800_000_000_000L

    private fun invitation(saved: MutableList<ReviewHistory> = mutableListOf(), asked: MutableList<Int> = mutableListOf(),
                           initial: ReviewHistory = ReviewHistory()) = ReviewInvitation(initial, { saved += it }) { asked += it }

    @Test fun `eligible at the fifth prompt`() {
        assertFalse(ReviewHistory(prompts = 4).eligible(now, "2.50.0"))
        assertTrue(ReviewHistory(prompts = 5).eligible(now, "2.50.0"))
    }

    @Test fun `the cooldown outlives versions and needs five more prompts`() {
        val asked = ReviewHistory(prompts = 5).recordingRequest(now, "2.50.0").copy(prompts = 10)
        assertFalse(asked.eligible(now + 120 * day - 1, "2.51.0"))
        assertFalse(asked.eligible(now + 120 * day, "2.50.0"))
        assertTrue(asked.eligible(now + 120 * day, "2.51.0"))
        assertFalse(asked.copy(prompts = 9).eligible(now + 120 * day, "2.51.0"))
    }

    @Test fun `at most two requests in a rolling year, and a version is asked once ever`() {
        val twice = ReviewHistory(prompts = 5).recordingRequest(now, "2.50.0").copy(prompts = 10)
            .recordingRequest(now + 120 * day, "2.51.0").copy(prompts = 15)
        assertFalse(twice.eligible(now + 364 * day, "2.52.0"))
        assertTrue(twice.eligible(now + 365 * day, "2.52.0"))
        val nextYear = ReviewHistory(prompts = 5).recordingRequest(now, "2.50.0").copy(prompts = 10)
            .recordingRequest(now + 366 * day, "2.51.0").copy(prompts = 15)
        assertFalse(nextYear.eligible(now + 486 * day, "2.50.0"))
    }

    @Test fun `the fifth prompt opens an opportunity, across relaunches`() {
        val saved = mutableListOf<ReviewHistory>()
        val first = invitation(saved)
        repeat(4) { first.sent("2.50.0", now) }
        assertNull(first.opportunity.value)
        val relaunched = invitation(initial = saved.last())
        assertNull(relaunched.opportunity.value)
        relaunched.sent("2.50.0", now)
        assertNotNull(relaunched.opportunity.value)
    }

    @Test fun `a request uses the opportunity and the cooldown, and reports the prompt count`() {
        val saved = mutableListOf<ReviewHistory>()
        val asked = mutableListOf<Int>()
        val invitation = invitation(saved, asked)
        repeat(5) { invitation.sent("2.50.0", now) }
        val missed = requireNotNull(invitation.opportunity.value)
        invitation.sent("2.50.0", now)
        val next = requireNotNull(invitation.opportunity.value)
        assertFalse(invitation.request(missed, "2.50.0", now))
        assertTrue(invitation.request(next, "2.50.0", now))
        assertNull(invitation.opportunity.value)
        assertEquals(listOf(6), asked)
        assertEquals(setOf("2.50.0"), saved.last().requestedVersions)
        invitation.sent("2.50.0", now)
        assertNull(invitation.opportunity.value)
    }

    @Test fun `a skipped or stale opportunity leaves the next prompt to ask`() {
        val asked = mutableListOf<Int>()
        val invitation = invitation(asked = asked)
        repeat(5) { invitation.sent("2.50.0", now) }
        val skipped = requireNotNull(invitation.opportunity.value)
        invitation.skip(skipped)
        assertFalse(invitation.request(skipped, "2.50.0", now))
        invitation.sent("2.50.0", now)
        val stale = requireNotNull(invitation.opportunity.value)
        assertFalse(invitation.request(stale, "2.50.0", now + 5_001))
        assertEquals(emptyList<Int>(), asked)
        invitation.sent("2.50.0", now + 6_000)
        assertTrue(invitation.request(requireNotNull(invitation.opportunity.value), "2.50.0", now + 7_000))
    }
}
