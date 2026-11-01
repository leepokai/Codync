package com.codync.android.core

/**
 * Bare web addresses in message text (`https://…`, `http://…`, `www.…`), found by the same rule
 * as the iPhone, desktop and terminal: the address runs to a space, quote, angle bracket,
 * backtick or CJK punctuation, without the sentence's closing punctuation or an unmatched
 * closing bracket. A port of CodyncUI `Autolink.swift`.
 */
object Autolink {
    /** One address: where it is in the text, and what it opens (`www.` addresses over https). */
    data class Found(val range: IntRange, val url: String)

    private val prefixes = listOf("https://", "http://", "www.")
    private const val TRAILING = ".,:;!?'\"*_~"

    /** The addresses in `text`, in order. */
    fun find(text: String): List<Found> = buildList {
        var i = 0
        while (i < text.length) {
            val length = length(text, i)
            if (length == 0) { i++; continue }
            val url = text.substring(i, i + length)
            add(Found(i until i + length, if (url.startsWith("www.", ignoreCase = true)) "https://$url" else url))
            i += length
        }
    }

    /** The length of the address starting at `at`, 0 for none. An address doesn't start inside a word. */
    private fun length(text: String, at: Int): Int {
        if (at > 0 && (text[at - 1].isLetterOrDigit() || text[at - 1] == '_')) return 0
        val prefix = prefixes.firstOrNull { text.startsWith(it, at, ignoreCase = true) } ?: return 0
        var end = (at until text.length).firstOrNull { stops(text[it]) } ?: text.length
        while (end > at) {
            val last = text[end - 1]
            val url = text.substring(at, end)
            val unmatched = when (last) {
                ')' -> url.count { it == ')' } > url.count { it == '(' }
                ']' -> url.count { it == ']' } > url.count { it == '[' }
                else -> false
            }
            if (last in TRAILING || unmatched) end-- else break
        }
        val body = at + prefix.length
        return if (end > body && (body until end).any { text[it].isLetterOrDigit() }) end - at else 0
    }

    private fun stops(c: Char) = c.isWhitespace() || c in "<>\"`" || c in '　'..'〿' || c in '＀'..'￯'
}
