package com.codync.android

import android.content.Context
import androidx.activity.ComponentActivity
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.Lifecycle
import com.codync.android.core.PendingSend
import com.codync.android.core.ReviewHistory
import com.codync.android.core.ReviewInvitation
import com.codync.android.core.WireJson
import com.google.android.play.core.review.testing.FakeReviewManager
import java.util.concurrent.atomic.AtomicInteger
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test

/** The prompts this phone's saved review history counts. */
internal fun savedReviewPrompts(context: Context): Int =
    context.getSharedPreferences("codync-review", Context.MODE_PRIVATE).getString("reviewInvitation.v1", null)
        ?.let { WireJson.decodeFromString(ReviewHistory.serializer(), it).prompts } ?: 0

/** The ask itself, through Play's own stand-in (a debug build isn't installed from Play). */
class ReviewPromptTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()
    private val requested = mutableListOf<Int>()
    private val asked = AtomicInteger()
    private val invitation = ReviewInvitation(ReviewHistory(prompts = 4), {}) { requested += it }

    private fun mount() = compose.setContent { ReviewInvitationEffect(invitation) { asked.incrementAndGet(); FakeReviewManager(it) } }
    private fun prompt() = invitation.sent(APP_RELEASE, System.currentTimeMillis())

    @Test fun theFifthPromptAsksGooglePlayOnce() {
        mount()
        prompt()
        compose.waitUntil(5_000) { asked.get() == 1 }
        assertEquals(listOf(5), requested)
        prompt()
        assertNull(invitation.opportunity.value)
    }

    @Test fun aPromptThatLandsInTheBackgroundNeverAsks() {
        mount()
        compose.activityRule.scenario.moveToState(Lifecycle.State.STARTED)
        prompt()
        compose.waitUntil(5_000) { invitation.opportunity.value == null }
        compose.activityRule.scenario.moveToState(Lifecycle.State.RESUMED)
        compose.waitForIdle()
        assertEquals(emptyList<Int>(), requested)
        assertEquals(0, asked.get())
    }

    @Test fun whatWasSaidOnACallNeverCounts() {
        val context = compose.activity
        val before = savedReviewPrompts(context)
        ReviewPrompt.promptSent(context, PendingSend("n1", "b1", "said aloud", 0, call = "c1"))
        assertEquals(before, savedReviewPrompts(context))
        ReviewPrompt.promptSent(context, PendingSend("n2", "b1", "typed", 0))
        assertEquals(before + 1, savedReviewPrompts(context))
    }
}
