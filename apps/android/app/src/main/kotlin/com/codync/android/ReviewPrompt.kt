package com.codync.android

import android.app.Activity
import android.content.Context
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.edit
import com.codync.android.core.Entry
import com.codync.android.core.LinkState
import com.codync.android.core.ReviewCancellation
import com.codync.android.core.ReviewHistory
import com.codync.android.core.ReviewInvitation
import com.codync.android.core.WireJson
import com.google.android.play.core.review.ReviewManagerFactory
import com.google.android.play.core.ktx.launchReview
import com.google.android.play.core.ktx.requestReview
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * This phone's invitation to rate Codync on Google Play (iOS `ReviewInvitation.shared` and its roster modifier): kept in
 * the app's preferences, never sent to a computer. Google Play decides whether its prompt actually shows.
 */
internal object ReviewPrompt {
    private const val KEY = "reviewInvitation.v1"
    private var invitation: ReviewInvitation? = null
    private val mutableOpportunity = MutableStateFlow<Long?>(null)
    val opportunity = mutableOpportunity.asStateFlow()

    private fun of(context: Context): ReviewInvitation = invitation ?: run {
        val prefs = context.applicationContext.getSharedPreferences("codync-review", Context.MODE_PRIVATE)
        val now = System.currentTimeMillis()
        val saved = prefs.getString(KEY, null)?.let { runCatching { WireJson.decodeFromString(ReviewHistory.serializer(), it) }.getOrNull() }
        ReviewInvitation(saved ?: ReviewHistory(firstUse = now), { history ->
            prefs.edit { putString(KEY, WireJson.encodeToString(ReviewHistory.serializer(), history)) }
        }) { event ->
            Analytics.capture(when (event) {
                is ReviewInvitation.Event.Opportunity -> AnalyticsEvent.ReviewOpportunity(event.completed)
                is ReviewInvitation.Event.Cancelled -> AnalyticsEvent.ReviewCancelled(event.reason)
                is ReviewInvitation.Event.Requested -> AnalyticsEvent.ReviewRequested(event.completed)
            })
        }.also { invitation = it }
    }

    private fun update(context: Context, change: ReviewInvitation.() -> Unit) {
        val shown = of(context)
        shown.change()
        mutableOpportunity.value = shown.opportunity
    }

    fun setActive(context: Context, active: Boolean) = update(context) { setActive(active, System.currentTimeMillis()) }
    fun sent(context: Context, nonce: String) = update(context) { sent(nonce, System.currentTimeMillis()) }
    fun viewed(context: Context, chat: String, entries: List<Entry>) = update(context) { viewed(chat, entries, System.currentTimeMillis()) }
    fun returned(context: Context, chat: String) = update(context) { returned(chat, APP_RELEASE, System.currentTimeMillis()) }
    fun cancel(context: Context, reason: ReviewCancellation) = update(context) { cancel(reason) }
    fun interrupt(context: Context, token: Any, active: Boolean) = update(context) { interrupt(token, active) }

    /** Uses up the opportunity (and this app's own cooldown) before Google Play is asked. */
    fun request(context: Context, token: Long): Boolean {
        var asked = false
        update(context) { asked = request(token, APP_RELEASE, System.currentTimeMillis()) }
        return asked
    }
}

/** A computer steady enough to read an answer on or to invite a rating over: connected, current, nothing else showing. */
internal val AppState.settledForReview: Boolean
    get() = actualConnection is LinkState.Ready && mismatch == null && error == null && screenRequest == null

/** Leaving [openChat] (the open chat's ID, null on the roster) may invite a rating; opening one turns a waiting invitation down. */
@Composable internal fun ReviewOnReturn(openChat: String?) {
    val context = LocalContext.current
    var lastChat by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(openChat) {
        if (openChat != null) ReviewPrompt.cancel(context, ReviewCancellation.Interaction)
        else lastChat?.let { ReviewPrompt.returned(context, it) }
        lastChat = openChat
    }
}

/** While [active] (the composer in use, files picked, dictation), reading a result doesn't count and no invitation shows. */
@Composable internal fun ReviewInterruption(active: Boolean) {
    val context = LocalContext.current
    val token = remember { Any() }
    DisposableEffect(active) {
        ReviewPrompt.interrupt(context, token, active)
        onDispose { ReviewPrompt.interrupt(context, token, false) }
    }
}

/** A finished answer at the end of [chat] that stays in sight for 3 s, with nothing over it, counts as read. */
@Composable internal fun ReviewReading(chat: String, entries: List<Entry>, ready: Boolean) {
    val context = LocalContext.current
    LaunchedEffect(chat, entries, ready) {
        if (!ready) return@LaunchedEffect
        delay(3_000)
        ReviewPrompt.viewed(context, chat, entries)
    }
}

/**
 * Back on the roster after reading an answer: once nothing has covered it for 2 s, Google Play's review prompt
 * ([blocked]: a chat, a sheet, an error, a computer that isn't settled cancels it).
 */
@Composable internal fun ReviewInvitationEffect(blocked: Boolean) {
    val context = LocalContext.current
    val token by ReviewPrompt.opportunity.collectAsState()
    LaunchedEffect(token, blocked) {
        val opportunity = token ?: return@LaunchedEffect
        if (blocked) { ReviewPrompt.cancel(context, ReviewCancellation.Unavailable); return@LaunchedEffect }
        delay(2_000)
        val activity = context as? Activity ?: return@LaunchedEffect ReviewPrompt.cancel(context, ReviewCancellation.Unavailable)
        if (!ReviewPrompt.request(context, opportunity)) return@LaunchedEffect
        try {
            val manager = ReviewManagerFactory.create(context)
            manager.launchReview(activity, manager.requestReview())
        } catch (cancelled: CancellationException) { throw cancelled }
        catch (error: Exception) { android.util.Log.i("Codync", "Review prompt unavailable: ${error.message}") }
    }
}
