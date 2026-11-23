package com.codync.android.core

/**
 * Find in the open conversation (iOS `ConversationFind`): the text searched, where it occurs in the messages loaded,
 * the match shown and stepping between matches. [shown] is kept by identity, so earlier messages loading above don't
 * move it; null shows the first match.
 */
data class ConversationFind(val query: String = "", val shown: Match? = null, private val reported: String = "") {
    /** One place the text occurs: the message (its row id) and how many earlier matches that message holds. */
    data class Match(val id: String, val occurrence: Int)

    /** Where the shown match is among [matches] (null when there are none). */
    fun index(matches: List<Match>): Int? = if (matches.isEmpty()) null else shown?.let(matches::indexOf)?.takeIf { it >= 0 } ?: 0

    /** What the bar's count reads: `2/7`, nothing before anything was typed, [noResults] when nothing matched. */
    fun label(matches: List<Match>, noResults: String): String {
        if (query.isBlank()) return ""
        val index = index(matches) ?: return noResults
        return "${index + 1}/${matches.size}"
    }

    /** Holds the first of [matches] as the shown one, once there is one, so matches loading above don't move it. */
    fun pinned(matches: List<Match>): ConversationFind = if (shown == null && matches.isNotEmpty()) copy(shown = matches[0]) else this

    /** The next or previous match, wrapping around. */
    fun step(forward: Boolean, matches: List<Match>): ConversationFind {
        val index = index(matches) ?: return this
        return copy(shown = matches[(index + if (forward) 1 else matches.size - 1) % matches.size])
    }

    /** A new text starts again from its first match. */
    fun type(text: String) = copy(query = text, shown = null)

    /** Once per text (when it settles, is stepped through or the bar closes): the find with it marked reported, or null
     *  when there's nothing new to report. The caller reports only whether it found anything, never the text. */
    fun searched(): ConversationFind? = if (query.isBlank() || reported == query) null else copy(reported = query)

    companion object {
        /** Every place [query] occurs in [messages] (row id and text, top to bottom), ignoring case; overlapping places count once. */
        fun matches(messages: List<Pair<String, String>>, query: String): List<Match> {
            if (query.isBlank()) return emptyList()
            return messages.flatMap { (id, text) ->
                val found = mutableListOf<Match>()
                var at = text.indexOf(query, ignoreCase = true)
                while (at >= 0) {
                    found += Match(id, found.size)
                    at = text.indexOf(query, at + query.length, ignoreCase = true)
                }
                found
            }
        }
    }
}
