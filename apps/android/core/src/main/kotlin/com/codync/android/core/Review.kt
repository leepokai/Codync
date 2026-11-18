package com.codync.android.core

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.serialization.Serializable

// When to invite a rating (iOS `ReviewHistory`, `ReviewInvitation`): at the fifth prompt this phone sent that reached
// the computer or its mailbox. Device-local; a request is an attempt, never evidence of a rating. Times are epoch
// milliseconds.

private const val DAY = 86_400_000L
/** The prompt that brings the first request, and how many more each later one needs. */
private const val PROMPTS_PER_REQUEST = 5
/** With nothing on screen to ask over (a send from the background), the opportunity lapses: coming back later must not ask. */
private const val OPPORTUNITY_LIFETIME = 5_000L

/** The device-local policy's memory, saved as JSON. */
@Serializable data class ReviewHistory(
    val prompts: Int = 0,
    val requests: List<Request> = emptyList(),
    val requestedVersions: Set<String> = emptySet(),
) {
    @Serializable data class Request(val date: Long, val version: String, val prompts: Int)

    /** Five prompts, once per version, twice a year, 120 days and five more prompts apart. */
    fun eligible(now: Long, version: String): Boolean {
        if (prompts < PROMPTS_PER_REQUEST || version in requestedVersions || requests.count { now - it.date < 365 * DAY } >= 2) return false
        val last = requests.lastOrNull() ?: return true
        return now - last.date >= 120 * DAY && prompts - last.prompts >= PROMPTS_PER_REQUEST
    }

    fun recordingRequest(now: Long, version: String): ReviewHistory = copy(requestedVersions = requestedVersions + version,
        requests = requests.filter { now - it.date < 365 * DAY } + Request(now, version, prompts))
}

/**
 * The invitation's state (iOS `ReviewInvitation`): a prompt that makes the history eligible opens an opportunity, and
 * the screen asks the store with its token. [save] keeps the history; [requested] reports an attempt and its prompt count.
 */
class ReviewInvitation(initial: ReviewHistory, private val save: (ReviewHistory) -> Unit, private val requested: (Int) -> Unit) {
    private var history = initial
    private val mutableOpportunity = MutableStateFlow<Long?>(null)
    /** The current opportunity's token. */
    val opportunity: StateFlow<Long?> = mutableOpportunity.asStateFlow()
    private var opportunityAt = 0L
    private var tokens = 0L

    /** A prompt reached the computer or its mailbox. */
    @Synchronized fun sent(version: String, now: Long) {
        history = history.copy(prompts = history.prompts + 1)
        save(history)
        if (history.eligible(now, version)) { mutableOpportunity.value = ++tokens; opportunityAt = now }
    }

    /** Saved before the store is asked: a request it doesn't show still uses up this app's own cooldown. */
    @Synchronized fun request(token: Long, version: String, now: Long): Boolean {
        if (mutableOpportunity.value != token) return false
        mutableOpportunity.value = null
        if (now - opportunityAt > OPPORTUNITY_LIFETIME) return false
        history = history.recordingRequest(now, version)
        save(history)
        requested(history.prompts)
        return true
    }

    /** The app couldn't ask on this one (in the background, or its screen went away): only a later prompt asks. */
    @Synchronized fun skip(token: Long) { mutableOpportunity.compareAndSet(token, null) }
}
