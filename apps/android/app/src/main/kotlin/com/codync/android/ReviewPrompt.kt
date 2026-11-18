package com.codync.android

import android.app.Activity
import android.content.Context
import android.util.Log
import androidx.activity.compose.LocalActivity
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.edit
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import com.codync.android.core.PendingSend
import com.codync.android.core.ReviewHistory
import com.codync.android.core.ReviewInvitation
import com.codync.android.core.WireJson
import com.google.android.play.core.ktx.launchReview
import com.google.android.play.core.ktx.requestReview
import com.google.android.play.core.review.ReviewManager
import com.google.android.play.core.review.ReviewManagerFactory
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.filterNotNull

/**
 * This phone's invitation to rate Codync on Google Play (iOS `ReviewInvitation.shared` and its root modifier): kept in
 * the app's preferences, never sent to a computer. Google Play decides whether its window actually shows.
 */
internal object ReviewPrompt {
    private const val KEY = "reviewInvitation.v1"
    private var invitation: ReviewInvitation? = null

    @Synchronized fun of(context: Context): ReviewInvitation = invitation ?: run {
        val prefs = context.applicationContext.getSharedPreferences("codync-review", Context.MODE_PRIVATE)
        val saved = prefs.getString(KEY, null)?.let { stored ->
            runCatching { WireJson.decodeFromString(ReviewHistory.serializer(), stored) }
                .onFailure { Log.e("Codync", "Cannot read review history", it) }.getOrNull()
        }
        ReviewInvitation(saved ?: ReviewHistory(), { history ->
            prefs.edit { putString(KEY, WireJson.encodeToString(ReviewHistory.serializer(), history)) }
        }) { prompts -> Analytics.capture(AnalyticsEvent.ReviewRequested(prompts)) }.also { invitation = it }
    }

    /** [send] reached the computer or its mailbox (iOS `onPromptSent`); what was said on a call never counts. */
    fun promptSent(context: Context, send: PendingSend) {
        if (send.call == null) of(context).sent(APP_RELEASE, System.currentTimeMillis())
    }

    /** Shows the review window of [manager] (Google Play's, or a test's stand-in), when it has one to show. */
    suspend fun ask(activity: Activity, manager: ReviewManager) {
        try { manager.launchReview(activity, manager.requestReview()) }
        catch (cancelled: CancellationException) { throw cancelled }
        // No Google Play on this phone, or it can't be reached: nothing shows.
        catch (error: Exception) {
            Log.i("Codync", "Google Play's review window is unavailable: ${error.message}")
            Analytics.capture(AnalyticsEvent.ReviewRequestFailed(ReviewFailure.of(error)))
        }
    }
}

/** Asks Google Play a second after the prompt that made [invitation] due, over whichever screen is open. */
@Composable internal fun ReviewInvitationEffect(
    invitation: ReviewInvitation = ReviewPrompt.of(LocalContext.current),
    manager: (Context) -> ReviewManager = ReviewManagerFactory::create,
) {
    val activity = LocalActivity.current ?: return
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(invitation) {
        invitation.opportunity.filterNotNull().collectLatest { token ->
            // Every exit drops it, so a rebuilt root (account switch, Start over) never asks without a new prompt.
            try {
                delay(1_000)
                // In the background, only a later prompt asks.
                if (lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED) && invitation.request(token, APP_RELEASE, System.currentTimeMillis()))
                    ReviewPrompt.ask(activity, manager(activity))
            } finally { invitation.skip(token) }
        }
    }
}
