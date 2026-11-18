package com.codync.android.core

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

/** A release's highlights, shown once after updating to it (iOS `Announcement`, docs/features/whats-new.md). */
@Serializable
data class Announcement(
    val version: String,
    val features: List<Feature> = emptyList(),
    /** Smaller changes, text only: English interface phrases, translated in every catalog. */
    val notes: List<String> = emptyList(),
) {
    /**
     * One highlight: a short preview clip over its name and a sentence about it. [title] and [body] are English
     * interface phrases; [beta] puts a tag beside the title; [media] names its muted, looping clip (`<media>.mp4`
     * beside the file); [film] is where the full video plays.
     */
    @Serializable
    data class Feature(val title: String, val body: String, val beta: Boolean = false, val media: String? = null, val film: String? = null)

    companion object {
        /** `announcements.json`, shared with every client; newest first. */
        fun parse(text: String): List<Announcement> = Json.decodeFromString(text)

        /**
         * The ones to show on [current], in the file's order (newest first): newer than [seen] and not newer than
         * [current]. Without [seen], a phone set up before ([usedBefore]) counts from 0.0.0, a new install from [current].
         */
        fun due(all: List<Announcement>, seen: String?, current: String, usedBefore: Boolean): List<Announcement> {
            if (AppVersion.parse(current) == null) return emptyList()
            val since = seen ?: if (usedBefore) "0.0.0" else current
            return all.filter { AppVersion.isBelow(since, it.version) && !AppVersion.isBelow(current, it.version) }
        }
    }
}

/**
 * This run's What's new (iOS `WhatsNew`): [current] is saved at once when nothing is [due], else once the window is
 * [shown], which reports it.
 */
class WhatsNewRun(
    all: List<Announcement>, seen: String?, private val current: String, usedBefore: Boolean,
    private val save: (String) -> Unit, private val capture: () -> Unit,
) {
    /** Due on this run, newest first. */
    val due = Announcement.due(all, seen, current, usedBefore)
    private var recorded = false

    init {
        if (due.isEmpty()) record()
    }

    /** On screen: not again on this version. */
    fun shown() {
        if (recorded) return
        record()
        capture()
    }

    private fun record() {
        recorded = true
        save(current)
    }
}
